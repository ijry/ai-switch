use crate::database::repositories::account_repository::AccountRepository;
use crate::database::repositories::quota_snapshot_repository::QuotaSnapshotRepository;
use crate::error::AppError;
use crate::models::account::{
    OfficialAccount, OfficialAccountStatus, RecordAccountQuotaSnapshotOutcome,
    RecordAccountQuotaSnapshotRequest, RefreshAccountQuotaSnapshotRequest,
};
use crate::models::quota_snapshot::NewQuotaSnapshot;
use reqwest::header::{ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use std::env;
use std::time::Duration;

pub struct AccountService;

impl AccountService {
    pub async fn list_official_account_statuses(
        pool: &SqlitePool,
    ) -> Result<Vec<OfficialAccountStatus>, AppError> {
        let accounts = AccountRepository::list(pool).await?;
        let mut statuses = Vec::with_capacity(accounts.len());

        for account in accounts {
            let quota_snapshot = match &account.quota_snapshot_id {
                Some(id) => QuotaSnapshotRepository::get_optional(pool, id).await?,
                None => None,
            };
            statuses.push(OfficialAccountStatus {
                account,
                quota_snapshot,
            });
        }

        Ok(statuses)
    }

    pub async fn record_account_quota_snapshot(
        pool: &SqlitePool,
        request: RecordAccountQuotaSnapshotRequest,
    ) -> Result<RecordAccountQuotaSnapshotOutcome, AppError> {
        let account = AccountRepository::get(pool, &request.account_id).await?;
        let status = request.status.trim();
        if !matches!(status, "ok" | "warning" | "error" | "unknown") {
            return Err(AppError::Validation {
                code: "validation.quota_status",
                message: "Quota status must be ok, warning, error, or unknown".to_string(),
                details: Some(request.status),
                recoverable: true,
            });
        }

        let summary_json = normalize_json(
            &request.summary_json,
            "validation.quota_summary_json",
            "Quota summary JSON is invalid",
        )?;
        let raw_excerpt_json = normalize_json(
            &request.raw_excerpt_json,
            "validation.quota_raw_excerpt_json",
            "Quota raw excerpt JSON is invalid",
        )?;

        let quota_snapshot = QuotaSnapshotRepository::insert(
            pool,
            NewQuotaSnapshot {
                owner_type: "official_account".to_string(),
                owner_id: account.id.clone(),
                status: status.to_string(),
                remaining_label: request
                    .remaining_label
                    .and_then(|value| non_empty_string(value.trim())),
                reset_at: request
                    .reset_at
                    .and_then(|value| non_empty_string(value.trim())),
                summary_json,
                raw_excerpt_json,
            },
        )
        .await?;
        let account =
            AccountRepository::update_quota_snapshot_id(pool, &account.id, &quota_snapshot.id)
                .await?;

        Ok(RecordAccountQuotaSnapshotOutcome {
            account,
            quota_snapshot,
        })
    }

    pub async fn refresh_account_quota_snapshot(
        pool: &SqlitePool,
        request: RefreshAccountQuotaSnapshotRequest,
    ) -> Result<RecordAccountQuotaSnapshotOutcome, AppError> {
        let account = AccountRepository::get(pool, &request.account_id).await?;
        let config = parse_quota_query_config(&account)?;
        let body = fetch_quota_query(&config).await?;

        Self::record_refreshed_quota_snapshot(pool, &account, &body).await
    }

    async fn record_refreshed_quota_snapshot(
        pool: &SqlitePool,
        account: &OfficialAccount,
        response_body: &str,
    ) -> Result<RecordAccountQuotaSnapshotOutcome, AppError> {
        let quota = quota_snapshot_from_response(account, response_body)?;
        let quota_snapshot = QuotaSnapshotRepository::insert(pool, quota).await?;
        let account =
            AccountRepository::update_quota_snapshot_id(pool, &account.id, &quota_snapshot.id)
                .await?;

        Ok(RecordAccountQuotaSnapshotOutcome {
            account,
            quota_snapshot,
        })
    }
}

#[derive(Debug, Deserialize)]
struct QuotaMetadata {
    quota_query: Option<QuotaQueryConfig>,
}

#[derive(Debug, Deserialize)]
struct QuotaQueryConfig {
    endpoint_url: String,
    auth_env_key: Option<String>,
    auth_scheme: Option<String>,
}

fn parse_quota_query_config(account: &OfficialAccount) -> Result<QuotaQueryConfig, AppError> {
    let metadata: QuotaMetadata =
        serde_json::from_str(&account.account_metadata_json).map_err(|error| {
            AppError::Validation {
                code: "validation.quota_query_config",
                message: "Account metadata JSON must contain a valid quota_query object"
                    .to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            }
        })?;

    let config = metadata.quota_query.ok_or_else(|| AppError::Validation {
        code: "validation.quota_query_config",
        message: "Account metadata must include quota_query before quota can be refreshed"
            .to_string(),
        details: Some("quota_query is missing".to_string()),
        recoverable: true,
    })?;

    validate_quota_endpoint(&config.endpoint_url)?;
    if let Some(auth_env_key) = &config.auth_env_key {
        validate_env_key(auth_env_key)?;
    }
    if let Some(auth_scheme) = &config.auth_scheme {
        if auth_scheme.contains('\r') || auth_scheme.contains('\n') {
            return Err(AppError::Validation {
                code: "validation.quota_query_auth",
                message: "Quota auth scheme cannot contain line breaks".to_string(),
                details: Some(auth_scheme.clone()),
                recoverable: true,
            });
        }
    }

    Ok(config)
}

fn validate_quota_endpoint(endpoint_url: &str) -> Result<(), AppError> {
    let parsed = reqwest::Url::parse(endpoint_url).map_err(|error| AppError::Validation {
        code: "validation.quota_query_url",
        message: "Quota endpoint URL is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;

    if parsed.scheme() != "https" {
        return Err(AppError::Validation {
            code: "validation.quota_query_url",
            message: "Quota endpoint URL must use https".to_string(),
            details: Some(endpoint_url.to_string()),
            recoverable: true,
        });
    }

    Ok(())
}

fn validate_env_key(env_key: &str) -> Result<(), AppError> {
    let mut chars = env_key.chars();
    let first = chars.next().ok_or_else(|| AppError::Validation {
        code: "validation.quota_query_auth_env",
        message: "Quota auth environment variable name cannot be empty".to_string(),
        details: None,
        recoverable: true,
    })?;

    if first.is_ascii_digit() || !is_env_key_char(first) || !chars.all(is_env_key_char) {
        return Err(AppError::Validation {
            code: "validation.quota_query_auth_env",
            message: "Quota auth environment variable name is invalid".to_string(),
            details: Some(env_key.to_string()),
            recoverable: true,
        });
    }

    Ok(())
}

fn is_env_key_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

async fn fetch_quota_query(config: &QuotaQueryConfig) -> Result<String, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|error| AppError::Adapter {
            code: "network.quota_query",
            message: "Could not create quota query client".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    let mut request = client
        .get(&config.endpoint_url)
        .header(ACCEPT, "application/json");
    if let Some(auth_env_key) = &config.auth_env_key {
        let token = env::var(auth_env_key).map_err(|error| AppError::Validation {
            code: "validation.quota_query_auth_env",
            message: "Quota auth environment variable is not available".to_string(),
            details: Some(format!("{auth_env_key}: {error}")),
            recoverable: true,
        })?;
        if token.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.quota_query_auth_env",
                message: "Quota auth environment variable is empty".to_string(),
                details: Some(auth_env_key.clone()),
                recoverable: true,
            });
        }
        let scheme = config.auth_scheme.as_deref().unwrap_or("Bearer");
        request = request.header(AUTHORIZATION, format!("{scheme} {token}"));
    }

    let response = request.send().await.map_err(|error| AppError::Adapter {
        code: "network.quota_query",
        message: "Quota endpoint request failed".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| AppError::Adapter {
        code: "network.quota_query",
        message: "Could not read quota endpoint response".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;

    if !status.is_success() {
        return Err(AppError::Adapter {
            code: "network.quota_query",
            message: "Quota endpoint returned an error status".to_string(),
            details: Some(format!("{status}: {}", trim_detail(&body))),
            recoverable: true,
        });
    }

    Ok(body)
}

fn quota_snapshot_from_response(
    account: &OfficialAccount,
    response_body: &str,
) -> Result<NewQuotaSnapshot, AppError> {
    let value: Value =
        serde_json::from_str(response_body).map_err(|error| AppError::Validation {
            code: "validation.quota_response_json",
            message: "Quota endpoint response must be valid JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .trim();
    if !matches!(status, "ok" | "warning" | "error" | "unknown") {
        return Err(AppError::Validation {
            code: "validation.quota_response_status",
            message: "Quota endpoint status must be ok, warning, error, or unknown".to_string(),
            details: Some(status.to_string()),
            recoverable: true,
        });
    }

    let remaining_label = optional_response_string(&value, "remaining_label")?;
    let reset_at = optional_response_string(&value, "reset_at")?;
    let summary = value.get("summary").cloned().unwrap_or_else(|| {
        json!({
            "status": status,
            "remaining_label": remaining_label,
            "reset_at": reset_at,
            "source": "quota_query"
        })
    });
    let summary = redact_sensitive_json(&summary);
    let raw_excerpt = redact_sensitive_json(&value);

    Ok(NewQuotaSnapshot {
        owner_type: "official_account".to_string(),
        owner_id: account.id.clone(),
        status: status.to_string(),
        remaining_label,
        reset_at,
        summary_json: summary.to_string(),
        raw_excerpt_json: raw_excerpt.to_string(),
    })
}

fn optional_response_string(value: &Value, key: &'static str) -> Result<Option<String>, AppError> {
    match value.get(key) {
        Some(Value::String(text)) => Ok(non_empty_string(text.trim())),
        Some(Value::Null) | None => Ok(None),
        Some(other) => Err(AppError::Validation {
            code: "validation.quota_response_json",
            message: "Quota endpoint optional fields must be strings when present".to_string(),
            details: Some(format!("{key}: {other}")),
            recoverable: true,
        }),
    }
}

fn redact_sensitive_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, value) in map {
                if is_sensitive_key(key) {
                    redacted.insert(key.clone(), Value::String("[redacted]".to_string()));
                } else {
                    redacted.insert(key.clone(), redact_sensitive_json(value));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_sensitive_json).collect()),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    [
        "token",
        "api_key",
        "apikey",
        "password",
        "secret",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn trim_detail(input: &str) -> String {
    const MAX_DETAIL_LEN: usize = 256;
    let mut chars = input.chars();
    let truncated: String = chars.by_ref().take(MAX_DETAIL_LEN).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        input.to_string()
    }
}

fn normalize_json(
    input: &str,
    code: &'static str,
    message: &'static str,
) -> Result<String, AppError> {
    let trimmed = input.trim();
    let normalized = if trimmed.is_empty() { "{}" } else { trimmed };
    serde_json::from_str::<serde_json::Value>(normalized).map_err(|error| {
        AppError::Validation {
            code,
            message: message.to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        }
    })?;

    Ok(normalized.to_string())
}

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::account::NewOfficialAccount;
    use crate::services::batch_service::BatchService;

    #[tokio::test]
    async fn record_account_quota_snapshot_links_snapshot_to_account_status() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account = BatchService::create_official_account(
            &pool,
            NewOfficialAccount {
                platform: "codex".to_string(),
                display_name: "Team Codex".to_string(),
                email: Some("team@example.com".to_string()),
                plan: Some("team".to_string()),
                account_metadata_json: "{}".to_string(),
                secret_ref: Some("secret://account/team".to_string()),
            },
            None,
        )
        .await
        .expect("account");

        let outcome = AccountService::record_account_quota_snapshot(
            &pool,
            RecordAccountQuotaSnapshotRequest {
                account_id: account.id.clone(),
                status: "warning".to_string(),
                remaining_label: Some("12% remaining".to_string()),
                reset_at: Some("2026-07-14T00:00:00Z".to_string()),
                summary_json: "{\"window\":\"daily\"}".to_string(),
                raw_excerpt_json: "{}".to_string(),
            },
        )
        .await
        .expect("quota");
        let statuses = AccountService::list_official_account_statuses(&pool)
            .await
            .expect("statuses");

        assert_eq!(outcome.quota_snapshot.status, "warning");
        assert_eq!(
            outcome.account.quota_snapshot_id.as_deref(),
            Some(outcome.quota_snapshot.id.as_str())
        );
        assert_eq!(statuses.len(), 1);
        assert_eq!(
            statuses[0]
                .quota_snapshot
                .as_ref()
                .map(|snapshot| snapshot.remaining_label.as_deref()),
            Some(Some("12% remaining"))
        );
    }

    #[tokio::test]
    async fn record_account_quota_snapshot_rejects_invalid_json() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account = BatchService::create_official_account(
            &pool,
            NewOfficialAccount {
                platform: "codex".to_string(),
                display_name: "Broken Quota".to_string(),
                email: None,
                plan: None,
                account_metadata_json: "{}".to_string(),
                secret_ref: None,
            },
            None,
        )
        .await
        .expect("account");

        let error = AccountService::record_account_quota_snapshot(
            &pool,
            RecordAccountQuotaSnapshotRequest {
                account_id: account.id,
                status: "ok".to_string(),
                remaining_label: None,
                reset_at: None,
                summary_json: "{".to_string(),
                raw_excerpt_json: "{}".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.quota_summary_json");
    }

    #[tokio::test]
    async fn record_refreshed_quota_snapshot_parses_and_redacts_response() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account = BatchService::create_official_account(
            &pool,
            NewOfficialAccount {
                platform: "codex".to_string(),
                display_name: "Refresh Quota".to_string(),
                email: None,
                plan: None,
                account_metadata_json:
                    "{\"quota_query\":{\"endpoint_url\":\"https://quota.example.com/status\"}}"
                        .to_string(),
                secret_ref: None,
            },
            None,
        )
        .await
        .expect("account");

        let outcome = AccountService::record_refreshed_quota_snapshot(
            &pool,
            &account,
            r#"{"status":"ok","remaining_label":"80% remaining","reset_at":"2026-07-15T00:00:00Z","summary":{"window":"daily"},"access_token":"raw-secret"}"#,
        )
        .await
        .expect("quota");

        assert_eq!(outcome.quota_snapshot.status, "ok");
        assert_eq!(
            outcome.quota_snapshot.remaining_label.as_deref(),
            Some("80% remaining")
        );
        assert_eq!(outcome.quota_snapshot.summary_json, r#"{"window":"daily"}"#);
        assert!(outcome
            .quota_snapshot
            .raw_excerpt_json
            .contains("[redacted]"));
        assert!(!outcome
            .quota_snapshot
            .raw_excerpt_json
            .contains("raw-secret"));
    }

    #[test]
    fn parse_quota_query_config_requires_https_metadata() {
        let account = OfficialAccount {
            id: "account-1".to_string(),
            platform: "codex".to_string(),
            display_name: "Unsafe".to_string(),
            email: None,
            plan: None,
            account_metadata_json:
                "{\"quota_query\":{\"endpoint_url\":\"http://quota.example.com/status\"}}"
                    .to_string(),
            secret_ref: None,
            quota_snapshot_id: None,
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        };

        let error = parse_quota_query_config(&account).expect_err("error");

        assert_eq!(error.code(), "validation.quota_query_url");
    }
}
