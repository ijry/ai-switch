use crate::database::repositories::sync_repository::SyncRepository;
use crate::error::AppError;
use crate::models::sync::{
    CreateSyncSnapshotRequest, NewSyncProfile, NewSyncSnapshot, SyncProfile, SyncSnapshot,
};
use chrono::Utc;
use serde_json::Value;
use sqlx::SqlitePool;

pub struct SyncService;

impl SyncService {
    pub async fn list_sync_profiles(pool: &SqlitePool) -> Result<Vec<SyncProfile>, AppError> {
        SyncRepository::list_profiles(pool).await
    }

    pub async fn create_sync_profile(
        pool: &SqlitePool,
        input: NewSyncProfile,
    ) -> Result<SyncProfile, AppError> {
        let normalized = normalize_sync_profile(input)?;
        SyncRepository::create_profile(pool, normalized).await
    }

    pub async fn list_sync_snapshots(pool: &SqlitePool) -> Result<Vec<SyncSnapshot>, AppError> {
        SyncRepository::list_snapshots(pool).await
    }

    pub async fn create_sync_snapshot(
        pool: &SqlitePool,
        request: CreateSyncSnapshotRequest,
    ) -> Result<SyncSnapshot, AppError> {
        let direction = normalize_direction(&request.direction)?;
        let profile_id = request
            .profile_id
            .and_then(|id| non_empty_string(id.trim().to_string()));
        let artifact_ref = request
            .artifact_ref
            .and_then(|id| non_empty_string(id.trim().to_string()));

        let item_counts_json = build_item_counts_json(pool).await?;
        let manifest_json = serde_json::json!({
            "schema": "ai-switch.sync.snapshot.v1",
            "direction": direction,
            "generated_at": Utc::now().to_rfc3339(),
            "item_counts": serde_json::from_str::<Value>(&item_counts_json).unwrap_or(Value::Null),
        })
        .to_string();

        SyncRepository::create_snapshot(
            pool,
            NewSyncSnapshot {
                profile_id,
                direction,
                status: "recorded".to_string(),
                item_counts_json,
                manifest_json,
                artifact_ref,
            },
        )
        .await
    }
}

fn normalize_sync_profile(input: NewSyncProfile) -> Result<NewSyncProfile, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.sync_name_required",
            message: "Sync profile name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let provider = input.provider.trim().to_lowercase();
    if !matches!(provider.as_str(), "local_folder" | "webdav" | "s3" | "git") {
        return Err(AppError::Validation {
            code: "validation.sync_provider",
            message: "Sync provider must be local_folder, webdav, s3, or git".to_string(),
            details: Some(input.provider),
            recoverable: true,
        });
    }

    let endpoint_url = input
        .endpoint_url
        .and_then(|value| non_empty_string(value.trim().to_string()));
    validate_sync_endpoint(&provider, endpoint_url.as_deref())?;

    let auth_ref = input
        .auth_ref
        .and_then(|value| non_empty_string(value.trim().to_string()));
    if let Some(auth_ref) = &auth_ref {
        if !is_secret_reference(auth_ref) {
            return Err(AppError::Validation {
                code: "validation.sync_auth_ref_required",
                message: "Sync auth values must use env:// or secret:// references".to_string(),
                details: None,
                recoverable: true,
            });
        }
    }

    let scope_json = normalize_scope_json(&input.scope_json)?;

    Ok(NewSyncProfile {
        name,
        provider,
        endpoint_url,
        auth_ref,
        scope_json,
        enabled: input.enabled,
        notes: input
            .notes
            .and_then(|notes| non_empty_string(notes.trim().to_string())),
    })
}

fn normalize_direction(direction: &str) -> Result<String, AppError> {
    let normalized = direction.trim().to_lowercase();
    if matches!(normalized.as_str(), "export" | "import") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.sync_direction",
        message: "Sync direction must be export or import".to_string(),
        details: Some(direction.to_string()),
        recoverable: true,
    })
}

fn normalize_scope_json(scope_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(scope_json, "{}", "validation.sync_scope_json")?;
    let Some(scope) = value.as_object() else {
        return Err(AppError::Validation {
            code: "validation.sync_scope_object",
            message: "Sync scope JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };

    serde_json::to_string(&Value::Object(scope.clone())).map_err(AppError::from)
}

fn validate_sync_endpoint(provider: &str, endpoint_url: Option<&str>) -> Result<(), AppError> {
    match provider {
        "local_folder" => Ok(()),
        "webdav" => {
            let Some(endpoint_url) = endpoint_url else {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_required",
                    message: "WebDAV sync profiles require an endpoint URL".to_string(),
                    details: None,
                    recoverable: true,
                });
            };
            if !endpoint_url.starts_with("http://") && !endpoint_url.starts_with("https://") {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_scheme",
                    message: "WebDAV endpoints must start with http:// or https://".to_string(),
                    details: Some(endpoint_url.to_string()),
                    recoverable: true,
                });
            }
            Ok(())
        }
        "s3" => {
            let Some(endpoint_url) = endpoint_url else {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_required",
                    message: "S3 sync profiles require an endpoint URL".to_string(),
                    details: None,
                    recoverable: true,
                });
            };
            if !endpoint_url.starts_with("s3://") {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_scheme",
                    message: "S3 endpoints must start with s3://".to_string(),
                    details: Some(endpoint_url.to_string()),
                    recoverable: true,
                });
            }
            Ok(())
        }
        "git" => {
            let Some(endpoint_url) = endpoint_url else {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_required",
                    message: "Git sync profiles require an endpoint URL".to_string(),
                    details: None,
                    recoverable: true,
                });
            };
            if !endpoint_url.starts_with("https://")
                && !endpoint_url.starts_with("ssh://")
                && !endpoint_url.starts_with("git@")
            {
                return Err(AppError::Validation {
                    code: "validation.sync_endpoint_scheme",
                    message: "Git endpoints must start with https://, ssh://, or git@".to_string(),
                    details: Some(endpoint_url.to_string()),
                    recoverable: true,
                });
            }
            Ok(())
        }
        _ => unreachable!("provider normalized before validation"),
    }
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
        message: "Sync JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

async fn build_item_counts_json(pool: &SqlitePool) -> Result<String, AppError> {
    let providers = count_rows(pool, "providers").await?;
    let accounts = count_rows(pool, "official_accounts").await?;
    let mcp_servers = count_rows(pool, "mcp_servers").await?;
    let prompt_assets = count_rows(pool, "prompt_assets").await?;
    let proxy_profiles = count_rows(pool, "proxy_profiles").await?;
    let failover_policies = count_rows(pool, "failover_policies").await?;
    let usage_events = count_rows(pool, "usage_events").await?;

    let item_counts_json = serde_json::json!({
        "providers": providers,
        "official_accounts": accounts,
        "mcp_servers": mcp_servers,
        "prompt_assets": prompt_assets,
        "proxy_profiles": proxy_profiles,
        "failover_policies": failover_policies,
        "usage_events": usage_events,
    })
    .to_string();

    Ok(item_counts_json)
}

async fn count_rows(pool: &SqlitePool, table: &str) -> Result<i64, AppError> {
    let sql = match table {
        "providers" => "SELECT COUNT(*) FROM providers",
        "official_accounts" => "SELECT COUNT(*) FROM official_accounts",
        "mcp_servers" => "SELECT COUNT(*) FROM mcp_servers",
        "prompt_assets" => "SELECT COUNT(*) FROM prompt_assets",
        "proxy_profiles" => "SELECT COUNT(*) FROM proxy_profiles",
        "failover_policies" => "SELECT COUNT(*) FROM failover_policies",
        "usage_events" => "SELECT COUNT(*) FROM usage_events",
        _ => unreachable!("table name is fixed"),
    };

    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .map_err(|err| AppError::Database {
            code: "database.sync_count",
            message: "Could not count sync items".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        })
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
    use crate::database::repositories::provider_repository::ProviderRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::provider::NewProvider;

    #[tokio::test]
    async fn create_sync_profile_normalizes_webdav_profile() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let profile = SyncService::create_sync_profile(
            &pool,
            NewSyncProfile {
                name: " Team WebDAV ".to_string(),
                provider: "WEBDAV".to_string(),
                endpoint_url: Some(" https://sync.example.com/ai-switch ".to_string()),
                auth_ref: Some(" env://WEBDAV_TOKEN ".to_string()),
                scope_json: "{\"providers\":true}".to_string(),
                enabled: true,
                notes: Some(" Shared export ".to_string()),
            },
        )
        .await
        .expect("profile");

        assert_eq!(profile.name, "Team WebDAV");
        assert_eq!(profile.provider, "webdav");
        assert_eq!(
            profile.endpoint_url.as_deref(),
            Some("https://sync.example.com/ai-switch")
        );
        assert_eq!(profile.auth_ref.as_deref(), Some("env://WEBDAV_TOKEN"));
        assert_eq!(profile.notes.as_deref(), Some("Shared export"));
    }

    #[tokio::test]
    async fn create_sync_profile_rejects_raw_auth() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = SyncService::create_sync_profile(
            &pool,
            NewSyncProfile {
                name: "Bad".to_string(),
                provider: "webdav".to_string(),
                endpoint_url: Some("https://sync.example.com".to_string()),
                auth_ref: Some("raw-token".to_string()),
                scope_json: "{}".to_string(),
                enabled: true,
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.sync_auth_ref_required");
    }

    #[tokio::test]
    async fn create_sync_snapshot_counts_current_tables() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: Some("https://api.example.com/v1".to_string()),
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");

        let snapshot = SyncService::create_sync_snapshot(
            &pool,
            CreateSyncSnapshotRequest {
                profile_id: None,
                direction: "EXPORT".to_string(),
                artifact_ref: None,
            },
        )
        .await
        .expect("snapshot");

        assert_eq!(snapshot.direction, "export");
        assert!(snapshot.manifest_json.contains("\"providers\":1"));
    }
}
