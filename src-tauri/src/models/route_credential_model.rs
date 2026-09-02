use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Healthy: selectable, no backoff. Rows in this state exist only transiently
/// (a success deletes them), so it is mostly the value synthesised for models
/// that have no row at all.
pub const MODEL_STATUS_OK: &str = "ok";
/// Set automatically when a semantic failure streak reaches the account's
/// `semantic_error_threshold`. Hard-excluded from selection, never probed.
pub const MODEL_STATUS_ERROR: &str = "error";
/// Set only by the user. Survives success, scheduled recovery and account-level
/// reactivation — automation must not override an explicit human decision.
pub const MODEL_STATUS_PAUSED: &str = "paused";

/// Per-(account, model) failure state. Mirrors the account-level columns on
/// `route_credentials` minus the redundant second timestamp: the account level
/// writes `next_retry_at` and `cooldown_until` the same value, so one suffices.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq)]
pub struct RouteCredentialModelState {
    pub route_credential_id: String,
    pub model_key: String,
    pub status: String,
    pub transient_failure_count: i64,
    pub cooldown_until: Option<String>,
    pub semantic_failure_streak_count: i64,
    pub semantic_failure_streak_fingerprint: Option<String>,
    pub last_failure_kind: Option<String>,
    pub last_failure_message: Option<String>,
    pub last_failure_response_json: Option<String>,
    /// Client-facing aliases pointing at this upstream model. Empty when the
    /// mapping was removed while the row lived on. Filled by the service layer,
    /// never stored.
    #[sqlx(skip)]
    #[serde(default)]
    pub aliases: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Where a failure should be charged.
///
/// `siblings` is every model key this account is known to serve, so the
/// repository can tell whether parking this one leaves nothing usable and the
/// account itself should back off. The service layer computes it — parsing
/// `model_mappings` is not the repository's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureScope<'a> {
    Account,
    Model {
        key: &'a str,
        siblings: &'a [String],
    },
}
