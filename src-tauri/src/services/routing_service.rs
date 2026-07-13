use crate::database::repositories::routing_repository::RoutingRepository;
use crate::error::AppError;
use crate::models::routing::{
    FailoverPolicy, NewFailoverPolicy, NewProxyProfile, NewUsageEvent, ProxyProfile, UsageEvent,
};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct RoutingService;

impl RoutingService {
    pub async fn list_proxy_profiles(pool: &SqlitePool) -> Result<Vec<ProxyProfile>, AppError> {
        RoutingRepository::list_proxy_profiles(pool).await
    }

    pub async fn create_proxy_profile(
        pool: &SqlitePool,
        input: NewProxyProfile,
    ) -> Result<ProxyProfile, AppError> {
        let normalized = normalize_proxy_profile(input)?;
        RoutingRepository::create_proxy_profile(pool, normalized).await
    }

    pub async fn list_failover_policies(
        pool: &SqlitePool,
    ) -> Result<Vec<FailoverPolicy>, AppError> {
        RoutingRepository::list_failover_policies(pool).await
    }

    pub async fn create_failover_policy(
        pool: &SqlitePool,
        input: NewFailoverPolicy,
    ) -> Result<FailoverPolicy, AppError> {
        let normalized = normalize_failover_policy(input)?;
        RoutingRepository::create_failover_policy(pool, normalized).await
    }

    pub async fn list_usage_events(pool: &SqlitePool) -> Result<Vec<UsageEvent>, AppError> {
        RoutingRepository::list_usage_events(pool).await
    }

    pub async fn create_usage_event(
        pool: &SqlitePool,
        input: NewUsageEvent,
    ) -> Result<UsageEvent, AppError> {
        let normalized = normalize_usage_event(input)?;
        RoutingRepository::create_usage_event(pool, normalized).await
    }
}

fn normalize_proxy_profile(input: NewProxyProfile) -> Result<NewProxyProfile, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.proxy_name_required",
            message: "Proxy profile name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let endpoint_url = input.endpoint_url.trim().to_string();
    if !has_allowed_proxy_scheme(&endpoint_url) {
        return Err(AppError::Validation {
            code: "validation.proxy_url_scheme",
            message: "Proxy URL must start with http://, https://, socks5://, or socks5h://"
                .to_string(),
            details: Some(endpoint_url),
            recoverable: true,
        });
    }
    if url_contains_credentials(&endpoint_url) {
        return Err(AppError::Validation {
            code: "validation.proxy_url_credentials",
            message: "Proxy credentials must be stored as env:// or secret:// references"
                .to_string(),
            details: None,
            recoverable: true,
        });
    }

    let auth_ref = input
        .auth_ref
        .and_then(|value| non_empty_string(value.trim().to_string()));
    if let Some(auth_ref) = &auth_ref {
        if !is_secret_reference(auth_ref) {
            return Err(AppError::Validation {
                code: "validation.proxy_auth_ref_required",
                message: "Proxy auth values must use env:// or secret:// references".to_string(),
                details: None,
                recoverable: true,
            });
        }
    }

    Ok(NewProxyProfile {
        name,
        endpoint_url,
        auth_ref,
        enabled: input.enabled,
        notes: input
            .notes
            .and_then(|notes| non_empty_string(notes.trim().to_string())),
    })
}

fn normalize_failover_policy(input: NewFailoverPolicy) -> Result<NewFailoverPolicy, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.failover_name_required",
            message: "Failover policy name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let strategy = input.strategy.trim().to_lowercase();
    if !matches!(strategy.as_str(), "ordered" | "round_robin") {
        return Err(AppError::Validation {
            code: "validation.failover_strategy",
            message: "Failover strategy must be ordered or round_robin".to_string(),
            details: Some(input.strategy),
            recoverable: true,
        });
    }

    let provider_ids_json = normalize_provider_ids_json(&input.provider_ids_json)?;

    Ok(NewFailoverPolicy {
        name,
        strategy,
        provider_ids_json,
        enabled: input.enabled,
        notes: input
            .notes
            .and_then(|notes| non_empty_string(notes.trim().to_string())),
    })
}

fn normalize_usage_event(input: NewUsageEvent) -> Result<NewUsageEvent, AppError> {
    if input.amount < 0 {
        return Err(AppError::Validation {
            code: "validation.usage_amount",
            message: "Usage amount must be zero or positive".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let metric_type = input.metric_type.trim().to_lowercase();
    if !matches!(
        metric_type.as_str(),
        "request" | "input_tokens" | "output_tokens" | "total_tokens" | "cost" | "quota"
    ) {
        return Err(AppError::Validation {
            code: "validation.usage_metric_type",
            message: "Usage metric type is not supported".to_string(),
            details: Some(input.metric_type),
            recoverable: true,
        });
    }

    let unit = input.unit.trim().to_string();
    if unit.is_empty() {
        return Err(AppError::Validation {
            code: "validation.usage_unit_required",
            message: "Usage unit is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let metadata_json = normalize_metadata_json(&input.metadata_json)?;

    Ok(NewUsageEvent {
        provider_id: input
            .provider_id
            .and_then(|id| non_empty_string(id.trim().to_string())),
        official_account_id: input
            .official_account_id
            .and_then(|id| non_empty_string(id.trim().to_string())),
        source_label: non_empty_string(input.source_label.trim().to_string())
            .unwrap_or_else(|| "manual".to_string()),
        metric_type,
        amount: input.amount,
        unit,
        metadata_json,
    })
}

fn normalize_provider_ids_json(provider_ids_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(
        provider_ids_json,
        "[]",
        "validation.failover_provider_ids_json",
    )?;
    let Some(provider_ids) = value.as_array() else {
        return Err(AppError::Validation {
            code: "validation.failover_provider_ids_array",
            message: "Failover provider IDs JSON must be an array".to_string(),
            details: None,
            recoverable: true,
        });
    };

    let mut normalized = Vec::with_capacity(provider_ids.len());
    for provider_id in provider_ids {
        let Some(provider_id) = provider_id.as_str() else {
            return Err(AppError::Validation {
                code: "validation.failover_provider_id_string",
                message: "Failover provider IDs must be strings".to_string(),
                details: None,
                recoverable: true,
            });
        };
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.failover_provider_id_required",
                message: "Failover provider IDs cannot be empty".to_string(),
                details: None,
                recoverable: true,
            });
        }
        normalized.push(provider_id.to_string());
    }

    serde_json::to_string(&normalized).map_err(AppError::from)
}

fn normalize_metadata_json(metadata_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(metadata_json, "{}", "validation.usage_metadata_json")?;
    let Some(metadata) = value.as_object() else {
        return Err(AppError::Validation {
            code: "validation.usage_metadata_object",
            message: "Usage metadata JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };

    for (key, value) in metadata {
        if is_sensitive_key(key) {
            let Some(value) = value.as_str() else {
                return Err(AppError::Validation {
                    code: "validation.usage_metadata_secret_ref_required",
                    message: "Sensitive usage metadata must use env:// or secret:// references"
                        .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            };

            if !is_secret_reference(value) {
                return Err(AppError::Validation {
                    code: "validation.usage_metadata_secret_ref_required",
                    message: "Sensitive usage metadata must use env:// or secret:// references"
                        .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            }
        }
    }

    serde_json::to_string(&value).map_err(AppError::from)
}

fn parse_json_or_default(
    json: &str,
    default_json: &str,
    code: &'static str,
) -> Result<Value, AppError> {
    let json = if json.trim().is_empty() {
        default_json
    } else {
        json.trim()
    };

    serde_json::from_str(json).map_err(|error| AppError::Validation {
        code,
        message: "Routing JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn has_allowed_proxy_scheme(endpoint_url: &str) -> bool {
    endpoint_url.starts_with("http://")
        || endpoint_url.starts_with("https://")
        || endpoint_url.starts_with("socks5://")
        || endpoint_url.starts_with("socks5h://")
}

fn url_contains_credentials(endpoint_url: &str) -> bool {
    let Some((_, rest)) = endpoint_url.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    authority.contains('@')
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_lowercase();
    ["token", "api_key", "apikey", "password", "secret"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn is_secret_reference(value: &str) -> bool {
    value.starts_with("env://") || value.starts_with("secret://")
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_proxy_profile_normalizes_safe_endpoint() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let profile = RoutingService::create_proxy_profile(
            &pool,
            NewProxyProfile {
                name: " Local Proxy ".to_string(),
                endpoint_url: " http://127.0.0.1:7890 ".to_string(),
                auth_ref: Some(" env://LOCAL_PROXY_AUTH ".to_string()),
                enabled: true,
                notes: Some(" Local only ".to_string()),
            },
        )
        .await
        .expect("profile");

        assert_eq!(profile.name, "Local Proxy");
        assert_eq!(profile.endpoint_url, "http://127.0.0.1:7890");
        assert_eq!(profile.auth_ref.as_deref(), Some("env://LOCAL_PROXY_AUTH"));
        assert_eq!(profile.notes.as_deref(), Some("Local only"));
    }

    #[tokio::test]
    async fn create_proxy_profile_rejects_raw_credentials() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutingService::create_proxy_profile(
            &pool,
            NewProxyProfile {
                name: "Unsafe".to_string(),
                endpoint_url: "http://user:password@127.0.0.1:7890".to_string(),
                auth_ref: None,
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.proxy_url_credentials");
    }

    #[tokio::test]
    async fn create_failover_policy_normalizes_provider_ids() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let policy = RoutingService::create_failover_policy(
            &pool,
            NewFailoverPolicy {
                name: " Primary then backup ".to_string(),
                strategy: "ORDERED".to_string(),
                provider_ids_json: "[\" provider-1 \",\"provider-2\"]".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect("policy");

        assert_eq!(policy.name, "Primary then backup");
        assert_eq!(policy.strategy, "ordered");
        assert_eq!(policy.provider_ids_json, "[\"provider-1\",\"provider-2\"]");
    }

    #[tokio::test]
    async fn create_failover_policy_rejects_non_string_ids() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutingService::create_failover_policy(
            &pool,
            NewFailoverPolicy {
                name: "Broken".to_string(),
                strategy: "ordered".to_string(),
                provider_ids_json: "[123]".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.failover_provider_id_string");
    }

    #[tokio::test]
    async fn create_usage_event_normalizes_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let event = RoutingService::create_usage_event(
            &pool,
            NewUsageEvent {
                provider_id: None,
                official_account_id: None,
                source_label: " manual ".to_string(),
                metric_type: "REQUEST".to_string(),
                amount: 5,
                unit: " count ".to_string(),
                metadata_json: "{\"window\":\"daily\"}".to_string(),
            },
        )
        .await
        .expect("event");

        assert_eq!(event.source_label, "manual");
        assert_eq!(event.metric_type, "request");
        assert_eq!(event.unit, "count");
        assert_eq!(event.metadata_json, "{\"window\":\"daily\"}");
    }

    #[tokio::test]
    async fn create_usage_event_rejects_raw_secret_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RoutingService::create_usage_event(
            &pool,
            NewUsageEvent {
                provider_id: None,
                official_account_id: None,
                source_label: "manual".to_string(),
                metric_type: "request".to_string(),
                amount: 1,
                unit: "count".to_string(),
                metadata_json: "{\"api_key\":\"raw-secret\"}".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(
            error.code(),
            "validation.usage_metadata_secret_ref_required"
        );
    }
}
