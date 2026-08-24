use crate::adapters::route_config::{
    codex_model_catalog_path, ClaudeEnvPlan, RouteConfigInput, TargetAdapter, TargetAdapterRegistry,
};
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
use crate::models::config_snapshot::ConfigWriteOutcome;
use crate::models::platform::{PlatformId, PlatformOperation};
use crate::models::route_credential::{
    ClaudeSlotWrite, ModelMapping, RouteCredentialPoolScope, CLAUDE_MODEL_SLOTS,
    CLAUDE_ONE_M_SUFFIX, CLAUDE_SUBAGENT_MODEL_ALIAS, FALLBACK_MODEL_ALIAS,
};
use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
use crate::paths::AppPaths;
use crate::services::config_write_service::{
    ConfigWriteCoordinator, ConfigWriteRequest, ConfigWriteRuntimeState,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_model_capability::{
    codex_model_catalog_payload, parse_model_capability,
};
use crate::services::settings_service::SettingsService;
use directories::BaseDirs;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub struct RouteConfigService;

impl RouteConfigService {
    pub async fn write_configs(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
        platform: &str,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;

        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;

        Self::write_configs_for_home(paths, pool, runtime, base_url, platform, &home).await
    }

    pub(crate) async fn write_configs_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
        platform: &str,
        home: &Path,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platform = PlatformId::parse(platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
        let adapter = route_config_adapter(platform)?;
        let platform_key = platform.as_str();

        let existing_route_proxy_key =
            RouteProxyKeyRepository::get_existing_platform_key(pool, platform_key).await?;
        let route_proxy_key = RouteProxyKeyRepository::ensure_platform_key(
            pool,
            platform_key,
            &generate_route_proxy_key(),
        )
        .await?;
        if platform == PlatformId::Codex {
            Self::write_codex_model_catalog(pool, home).await?;
        }
        let claude_env = Self::resolve_claude_env_plan(paths, pool, platform).await?;
        let request = ConfigWriteRequest {
            adapter,
            home: home.to_path_buf(),
            input: RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key: route_proxy_key.clone(),
                claude_env,
            },
        };
        match ConfigWriteCoordinator::write_group(paths, pool, runtime, vec![request]).await {
            Ok(outcomes) => Ok(outcomes),
            Err(error) => {
                if existing_route_proxy_key.is_none() {
                    let _ = RouteProxyKeyRepository::delete_if_matches(
                        pool,
                        platform_key,
                        &route_proxy_key,
                    )
                    .await;
                }
                Err(error)
            }
        }
    }

    /// Rewrites only platforms that already own a managed proxy key. This is
    /// used for HTTP/HTTPS changes and never creates additional client config.
    pub async fn write_existing_configs(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let home = resolve_home_dir()?;
        Self::write_existing_configs_for_home(paths, pool, runtime, base_url, &home).await
    }

    pub(crate) async fn write_existing_configs_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        base_url: &str,
        home: &Path,
    ) -> Result<Vec<ConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platforms = RouteProxyKeyRepository::list_platforms(pool).await?;
        let registry = TargetAdapterRegistry::new();
        let mut requests = Vec::with_capacity(platforms.len());
        let mut skipped = Vec::new();

        for platform in platforms {
            let parsed = match PlatformId::parse(&platform) {
                Ok(parsed) => parsed,
                Err(_) => {
                    skipped.push(skipped_outcome(
                        &platform,
                        &platform,
                        "config.adapter_unavailable",
                    ));
                    continue;
                }
            };
            if PlatformCapabilityService::require(parsed, PlatformOperation::ConfigWrite).is_err() {
                skipped.push(skipped_outcome(
                    &platform,
                    parsed.as_str(),
                    "config.adapter_unavailable",
                ));
                continue;
            }
            let Some(adapter) = registry.for_platform(parsed) else {
                skipped.push(skipped_outcome(
                    &platform,
                    parsed.as_str(),
                    "config.adapter_unavailable",
                ));
                continue;
            };
            let Some(route_proxy_key) =
                RouteProxyKeyRepository::get_existing_platform_key(pool, &platform).await?
            else {
                skipped.push(skipped_outcome(
                    adapter.target_key(),
                    parsed.as_str(),
                    "config.route_proxy_key_missing",
                ));
                continue;
            };
            if parsed == PlatformId::Codex {
                Self::write_codex_model_catalog(pool, home).await?;
            }
            // Per-iteration: a claude request must not inherit another
            // platform's pool state.
            let claude_env = Self::resolve_claude_env_plan(paths, pool, parsed).await?;
            requests.push(ConfigWriteRequest {
                adapter,
                home: home.to_path_buf(),
                input: RouteConfigInput {
                    base_url: base_url.to_string(),
                    route_proxy_key,
                    claude_env,
                },
            });
        }

        let mut outcomes =
            ConfigWriteCoordinator::write_group(paths, pool, runtime, requests).await?;
        let operation_id = outcomes
            .first()
            .map(|outcome| outcome.operation_id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        for outcome in &mut skipped {
            outcome.operation_id = operation_id.clone();
        }
        outcomes.extend(skipped);
        Ok(outcomes)
    }

    /// Whether writing config right now would change the file on disk.
    ///
    /// Renders through the real adapter and compares bytes, so this can never
    /// drift from what a write actually produces — a hand-rolled "did the pool
    /// change since last write" flag would.
    ///
    /// `false` when the platform has no managed proxy key yet (nothing was ever
    /// written, so there is nothing to re-write) and on any error: a stale-config
    /// hint must never be the thing that breaks the screen.
    pub async fn config_write_is_stale(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
    ) -> bool {
        let home = match resolve_home_dir() {
            Ok(home) => home,
            Err(_) => return false,
        };
        Self::config_write_is_stale_for_home(paths, pool, base_url, platform, &home).await
    }

    pub(crate) async fn config_write_is_stale_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
        home: &Path,
    ) -> bool {
        Self::rendered_config_differs(paths, pool, base_url, platform, home)
            .await
            .unwrap_or(false)
    }

    async fn rendered_config_differs(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
        home: &Path,
    ) -> Result<bool, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platform = PlatformId::parse(platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;

        // Never written for this platform → nothing to re-write.
        let Some(route_proxy_key) =
            RouteProxyKeyRepository::get_existing_platform_key(pool, platform.as_str()).await?
        else {
            return Ok(false);
        };

        let adapter = route_config_adapter(platform)?;
        let path = adapter.resolve_path(home);
        let existing = tokio::fs::read(&path).await.ok();
        if existing.is_none() {
            // The file we manage is gone; writing would recreate it.
            return Ok(true);
        }

        let claude_env = Self::resolve_claude_env_plan(paths, pool, platform).await?;
        let rendered = adapter.render(
            &path,
            existing.as_deref(),
            &RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key,
                claude_env,
            },
        )?;

        // Compare parsed content, not bytes: `render` pretty-prints whatever it
        // parsed, so a file the coordinator previously wrote compact would look
        // "stale" forever on formatting alone.
        Ok(config_content_differs(
            existing.as_deref().unwrap_or_default(),
            &rendered,
        ))
    }

    pub(crate) async fn write_existing_config_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
        home: &Path,
        base_url: &str,
        platform: &str,
        route_proxy_key: &str,
    ) -> Result<ConfigWriteOutcome, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platform = PlatformId::parse(platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::ConfigWrite)?;
        let claude_env = Self::resolve_claude_env_plan(paths, pool, platform).await?;
        let request = ConfigWriteRequest {
            adapter: route_config_adapter(platform)?,
            home: home.to_path_buf(),
            input: RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key: route_proxy_key.to_string(),
                claude_env,
            },
        };
        if platform == PlatformId::Codex {
            Self::write_codex_model_catalog(pool, home).await?;
        }
        ConfigWriteCoordinator::write_group(paths, pool, runtime, vec![request])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.route_config_write",
                message: "Configuration write returned no target outcome".to_string(),
                details: None,
                recoverable: false,
            })
    }

    /// Resolves one alias's write across the pool. Shared by the four `/model`
    /// slots, the subagent alias, and the fallback alias so all of them obey the
    /// same 1M and display-name rules.
    ///
    /// Always the generic alias, never an account's upstream model name: one
    /// settings file serves the whole pool, so the proxy does the per-account
    /// translation.
    ///
    /// Merge rules, both deliberately conservative because the client contract
    /// is one value while our data is per-account:
    /// - **1M is unanimous (AND).** Every account configuring the alias must
    ///   support it, else the suffix is omitted. Declaring it otherwise would
    ///   route a 1M-sized prompt to an account that cannot serve it.
    /// - **Display name needs consensus.** Accounts that set a non-empty label
    ///   must all agree; a disagreement omits the key rather than picking a
    ///   winner. Accounts leaving it blank abstain instead of vetoing.
    fn resolve_alias_write(account_mappings: &[Vec<ModelMapping>], alias: &str) -> ClaudeSlotWrite {
        let configured = account_mappings
            .iter()
            .filter_map(|mappings| {
                mappings
                    .iter()
                    .find(|mapping| mapping.from.trim() == alias && !mapping.to.trim().is_empty())
            })
            .collect::<Vec<_>>();

        if configured.is_empty() {
            return ClaudeSlotWrite::default();
        }

        let all_support_one_m = configured
            .iter()
            .all(|mapping| mapping.supports_1m == Some(true));
        let model = if all_support_one_m {
            format!("{alias}{CLAUDE_ONE_M_SUFFIX}")
        } else {
            alias.to_string()
        };

        let mut labels = configured
            .iter()
            .filter_map(|mapping| mapping.label.as_deref().map(str::trim))
            .filter(|label| !label.is_empty());
        let first = labels.next();
        let display_name = match first {
            Some(label) if labels.all(|other| other == label) => Some(label.to_string()),
            _ => None,
        };

        ClaudeSlotWrite {
            model: Some(model),
            display_name,
        }
    }

    fn resolve_claude_slot_writes(account_mappings: &[Vec<ModelMapping>]) -> Vec<ClaudeSlotWrite> {
        CLAUDE_MODEL_SLOTS
            .iter()
            .map(|slot| Self::resolve_alias_write(account_mappings, slot.alias))
            .collect()
    }

    /// Loads every enabled in-pool Claude API credential's mappings.
    ///
    /// Official credentials are excluded — their bodies are forwarded without
    /// model rewriting, so an alias would reach the vendor verbatim and 404.
    /// Like the Codex catalog, this does not filter on `status`, so a cooling
    /// account still counts.
    async fn claude_pool_mappings(pool: &SqlitePool) -> Result<Vec<Vec<ModelMapping>>, AppError> {
        let ids = RoutePoolRepository::list_member_ids(pool, PlatformId::Claude.as_str()).await?;
        let credentials = RouteCredentialRepository::list_by_ids(
            pool,
            &ids,
            &RouteCredentialSelectionContext {
                platform: PlatformId::Claude.as_str().to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await?;

        Ok(credentials
            .iter()
            .filter(|credential| credential.kind == "api")
            .map(|credential| parse_model_capability(&credential.config_json).mappings)
            .collect())
    }

    /// Parses the pool-wide client config, ignoring anything that is not a JSON
    /// object. A malformed value must not block a config write: the user would be
    /// locked out of writing any config until they fixed unrelated JSON.
    fn parse_claude_client_config(raw: Option<&str>) -> Option<Map<String, Value>> {
        let raw = raw.map(str::trim).filter(|value| !value.is_empty())?;
        let parsed = serde_json::from_str::<Value>(raw).ok()?;
        let object = parsed.as_object()?;
        (!object.is_empty()).then(|| object.clone())
    }

    /// Builds the Claude-only env plan for a config write. Non-Claude platforms
    /// get an empty plan, which clears every managed key.
    async fn resolve_claude_env_plan(
        paths: &AppPaths,
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<ClaudeEnvPlan, AppError> {
        if platform != PlatformId::Claude {
            return Ok(ClaudeEnvPlan::default());
        }

        let account_mappings = Self::claude_pool_mappings(pool).await?;
        let settings = SettingsService::load(paths).await?;
        Ok(ClaudeEnvPlan {
            subagent_model: Self::resolve_alias_write(
                &account_mappings,
                CLAUDE_SUBAGENT_MODEL_ALIAS,
            )
            .model,
            // Requests that fall outside the four `/model` roles. The client key
            // wants a concrete model, so write the fallback alias the proxy
            // already rewrites per account.
            fallback_model: Self::resolve_alias_write(&account_mappings, FALLBACK_MODEL_ALIAS)
                .model,
            slots: Self::resolve_claude_slot_writes(&account_mappings),
            client_config: Self::parse_claude_client_config(
                settings.claude_client_config_json.as_deref(),
            ),
        })
    }

    async fn write_codex_model_catalog(pool: &SqlitePool, home: &Path) -> Result<(), AppError> {
        let ids = RoutePoolRepository::list_member_ids(pool, PlatformId::Codex.as_str()).await?;
        let credentials = RouteCredentialRepository::list_by_ids(
            pool,
            &ids,
            &RouteCredentialSelectionContext {
                platform: PlatformId::Codex.as_str().to_string(),
                pool_scope: RouteCredentialPoolScope::InPool,
            },
        )
        .await?;
        let capabilities = credentials
            .iter()
            .map(|credential| parse_model_capability(&credential.config_json))
            .collect::<Vec<_>>();
        let payload = codex_model_catalog_payload(&capabilities);
        let bytes = serde_json::to_vec_pretty(&payload).map_err(|err| AppError::Validation {
            code: "validation.codex_model_catalog_serialization",
            message: "Could not serialize Codex model catalog".to_string(),
            details: Some(err.to_string()),
            recoverable: false,
        })?;
        let path = codex_model_catalog_path(home);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| AppError::Filesystem {
                    code: "filesystem.codex_model_catalog_dir",
                    message: "Could not create Codex configuration directory".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|err| AppError::Filesystem {
                code: "filesystem.codex_model_catalog_write",
                message: "Could not write Codex model catalog".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })
    }
}

fn skipped_outcome(target_key: &str, platform: &str, error_code: &str) -> ConfigWriteOutcome {
    ConfigWriteOutcome {
        operation_id: String::new(),
        snapshot_id: None,
        target_app_id: None,
        target_key: target_key.to_string(),
        platform: platform.to_string(),
        path: String::new(),
        status: "skipped".to_string(),
        before_hash: None,
        after_hash: None,
        error_code: Some(error_code.to_string()),
    }
}

fn route_config_adapter(platform: PlatformId) -> Result<Arc<dyn TargetAdapter>, AppError> {
    TargetAdapterRegistry::new()
        .for_platform(platform)
        .ok_or_else(|| AppError::Validation {
            code: "config.adapter_unavailable",
            message: "No verified native configuration adapter is available".to_string(),
            details: Some(platform.as_str().to_string()),
            recoverable: true,
        })
}

fn resolve_home_dir() -> Result<PathBuf, AppError> {
    BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })
}

/// Whether two config payloads differ in content. JSON is compared as parsed
/// values so formatting alone never reads as a pending change; anything else
/// (TOML) falls back to bytes.
fn config_content_differs(existing: &[u8], rendered: &[u8]) -> bool {
    match (
        serde_json::from_slice::<Value>(existing),
        serde_json::from_slice::<Value>(rendered),
    ) {
        (Ok(existing), Ok(rendered)) => existing != rendered,
        _ => existing != rendered,
    }
}

fn normalize_base_url(base_url: &str) -> Result<&str, AppError> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(AppError::Validation {
            code: "validation.route_proxy_base_url_required",
            message: "Route proxy base URL is required before writing configs".to_string(),
            details: None,
            recoverable: true,
        });
    }
    Ok(base_url)
}

pub fn generate_route_proxy_key() -> String {
    format!("sk-ai-switch-{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_writer::ConfigWriter;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::services::config_write_service::ConfigWriteRuntimeState;

    #[test]
    fn generated_route_proxy_key_uses_sk_shape() {
        let key = generate_route_proxy_key();
        assert!(key.starts_with("sk-ai-switch-"));
        assert!(key.len() > "sk-ai-switch-".len() + 20);
    }

    #[tokio::test]
    async fn write_configs_rejects_unsupported_platform_without_writing_all_targets() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = ConfigWriteRuntimeState::default();
        let error = RouteConfigService::write_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            "opencode",
            temp.path(),
        )
        .await
        .expect_err("unsupported target");

        match error {
            AppError::Validation { code, details, .. } => {
                assert_eq!(code, "capability.unavailable");
                assert_eq!(
                    details.as_deref(),
                    Some("capability.native_config_unavailable")
                );
            }
            other => panic!("expected validation error, got {other:?}"),
        }
        assert!(
            RouteProxyKeyRepository::get_existing_platform_key(&pool, "opencode")
                .await
                .expect("key lookup")
                .is_none()
        );
    }

    #[tokio::test]
    async fn every_route_config_entry_point_leaves_hermes_config_untouched() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().join("home");
        let paths = AppPaths::from_data_dir(fixture.path().join("app-data"));
        paths.ensure().await.unwrap();
        let pool = create_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        let runtime = ConfigWriteRuntimeState::default();
        let hermes = home.join(".hermes").join("config.yaml");
        tokio::fs::create_dir_all(hermes.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&hermes, b"model: sentinel\n")
            .await
            .unwrap();
        let before = ConfigWriter::inspect(&hermes).await.unwrap();

        let error = RouteConfigService::write_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            "hermes",
            &home,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            AppError::Validation {
                code: "capability.unavailable",
                ..
            }
        ));

        RouteProxyKeyRepository::ensure_platform_key(&pool, "hermes", "sk-ai-switch-hermes")
            .await
            .unwrap();
        let outcomes = RouteConfigService::write_existing_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            &home,
        )
        .await
        .unwrap();
        assert!(outcomes
            .iter()
            .any(|item| item.platform == "hermes" && item.status == "skipped"));

        let after = ConfigWriter::inspect(&hermes).await.unwrap();
        assert_eq!(after.hash, before.hash);
        assert_eq!(after.bytes, before.bytes);
    }

    #[tokio::test]
    async fn write_configs_removes_new_proxy_key_when_user_config_is_invalid() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let app_dir = tempfile::tempdir().expect("app dir");
        let home = tempfile::tempdir().expect("home dir");
        let codex_dir = home.path().join(".codex");
        tokio::fs::create_dir_all(&codex_dir).await.expect("mkdir");
        tokio::fs::write(codex_dir.join("config.toml"), "model_provider = [invalid")
            .await
            .expect("seed invalid config");

        let paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = ConfigWriteRuntimeState::default();

        let error = RouteConfigService::write_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            "codex",
            home.path(),
        )
        .await
        .expect_err("invalid config must fail");

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "filesystem.route_config_write",
                details: Some(details),
                ..
            } if details.contains("validation.route_config_existing_invalid")
        ));
        assert!(
            RouteProxyKeyRepository::get_existing_platform_key(&pool, "codex")
                .await
                .expect("key lookup")
                .is_none()
        );
    }

    #[tokio::test]
    async fn ensure_platform_key_is_stable_across_generations() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let first = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "grok",
            &generate_route_proxy_key(),
        )
        .await
        .expect("first");
        let second = RouteProxyKeyRepository::ensure_platform_key(
            &pool,
            "grok",
            &generate_route_proxy_key(),
        )
        .await
        .expect("second");

        assert_eq!(first, second);
        assert!(first.starts_with("sk-ai-switch-"));
        assert_eq!(
            RouteProxyKeyRepository::get_platform_by_key(&pool, &first)
                .await
                .expect("lookup")
                .as_deref(),
            Some("grok")
        );
    }

    #[tokio::test]
    async fn write_existing_configs_for_home_writes_only_preexisting_proxy_key_platforms() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let app_dir = tempfile::tempdir().expect("app dir");
        let home = tempfile::tempdir().expect("home dir");
        let paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let runtime = ConfigWriteRuntimeState::default();
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex key");

        let outcomes = RouteConfigService::write_existing_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "https://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect("write existing configs");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].target_key, "codex");
        assert_eq!(outcomes[0].status, "succeeded");
        assert!(outcomes[0].snapshot_id.is_some());
        assert!(serde_json::to_value(&outcomes[0])
            .unwrap()
            .get("route_proxy_key")
            .is_none());
        let codex_config = tokio::fs::read_to_string(home.path().join(".codex/config.toml"))
            .await
            .expect("codex config");
        assert!(codex_config.contains("https://127.0.0.1:43111"));
        assert!(codex_config.contains("model_catalog_json = \"ai-switch-model-catalog.json\""));
        let catalog: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(home.path().join(".codex/ai-switch-model-catalog.json"))
                .await
                .expect("codex model catalog"),
        )
        .expect("valid codex model catalog");
        assert!(catalog.get("models").is_some());
        assert!(catalog.get("data").is_none());
        assert!(!home.path().join(".claude/settings.json").exists());
        assert!(!home.path().join(".gemini/settings.json").exists());
        assert!(!home.path().join(".grok/settings.json").exists());
        assert_eq!(
            RouteProxyKeyRepository::list_platforms(&pool)
                .await
                .expect("platforms"),
            vec!["codex".to_string()]
        );
    }

    #[tokio::test]
    async fn write_existing_codex_config_preserves_unmanaged_toml() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        let codex_dir = home.path().join(".codex");
        tokio::fs::create_dir_all(&codex_dir).await.expect("mkdir");
        let codex_path = codex_dir.join("config.toml");
        tokio::fs::write(
            &codex_path,
            r#"approval_policy = "never"

[model_providers.keep]
name = "Keep"
base_url = "https://keep.example/v1"
wire_api = "chat"
api_key_env_var = "KEEP_KEY"

[mcp_servers.filesystem]
command = "npx"
"#,
        )
        .await
        .expect("seed config");

        RouteConfigService::write_existing_config_for_home(
            &paths,
            &pool,
            &runtime,
            home.path(),
            "http://127.0.0.1:43111",
            "codex",
            "sk-ai-switch-test",
        )
        .await
        .expect("write config");

        let written = tokio::fs::read_to_string(&codex_path)
            .await
            .expect("read config");
        assert!(written.contains("approval_policy = \"never\""));
        assert!(written.contains("[model_providers.keep]"));
        assert!(written.contains("api_key_env_var = \"KEEP_KEY\""));
        assert!(written.contains("[mcp_servers.filesystem]"));
        assert!(written.contains("model_provider = \"ai-switch\""));
        assert!(written.contains("[model_providers.ai-switch]"));
        assert!(written.contains("base_url = \"http://127.0.0.1:43111/v1\""));
        assert!(written.contains("experimental_bearer_token = \"sk-ai-switch-test\""));
        assert!(!written.contains("api_key = \"sk-ai-switch-test\""));
    }

    #[tokio::test]
    async fn write_existing_json_config_preserves_unmanaged_settings_and_env() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        let claude_dir = home.path().join(".claude");
        tokio::fs::create_dir_all(&claude_dir).await.expect("mkdir");
        let claude_path = claude_dir.join("settings.json");
        tokio::fs::write(
            &claude_path,
            r#"{
  "permissions": {
    "allow": ["Bash(ls)"]
  },
  "env": {
    "EXISTING_FLAG": "1",
    "ANTHROPIC_BASE_URL": "https://old.example"
  }
}"#,
        )
        .await
        .expect("seed settings");

        RouteConfigService::write_existing_config_for_home(
            &paths,
            &pool,
            &runtime,
            home.path(),
            "https://127.0.0.1:43111",
            "claude",
            "sk-ai-switch-test",
        )
        .await
        .expect("write settings");

        let written = tokio::fs::read_to_string(&claude_path)
            .await
            .expect("read settings");
        let json: serde_json::Value = serde_json::from_str(&written).expect("valid json");
        assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], "https://127.0.0.1:43111");
        // The key Claude Code actually authenticates with.
        assert_eq!(json["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-ai-switch-test");
        assert!(json["env"].get("AI_SWITCH_ROUTE_PROXY_API_KEY").is_none());
        assert_eq!(
            json["aiSwitch"]["routeProxy"]["baseUrl"],
            "https://127.0.0.1:43111"
        );
        assert_eq!(json["aiSwitch"]["routeProxy"]["platform"], "claude");
    }

    async fn seed_claude_pool_member(pool: &SqlitePool, kind: &str, mappings_json: &str) {
        use crate::models::route_credential::CreateApiRouteCredentialInput;
        use crate::models::route_pool::SetRoutePoolMembersInput;
        use crate::services::route_credential_service::RouteCredentialService;
        use crate::services::route_pool_service::RoutePoolService;

        let credential = RouteCredentialService::create_api(
            pool,
            CreateApiRouteCredentialInput {
                platform: "claude".to_string(),
                display_name: "Claude Account".to_string(),
                api_key: "sk-test".to_string(),
                base_url: "https://api.example.com".to_string(),
                interface_format: "anthropic".to_string(),
                model_mappings_json: mappings_json.to_string(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create claude credential");

        if kind != "api" {
            sqlx::query("UPDATE route_credentials SET kind = ? WHERE id = ?")
                .bind(kind)
                .bind(&credential.id)
                .execute(pool)
                .await
                .expect("set kind");
        }

        RoutePoolService::set_members(
            pool,
            SetRoutePoolMembersInput {
                platform: "claude".to_string(),
                account_ids: vec![credential.id],
            },
        )
        .await
        .expect("set pool members");
    }

    async fn write_claude_config_with_pool(
        home: &Path,
        paths: &AppPaths,
        pool: &SqlitePool,
        runtime: &ConfigWriteRuntimeState,
    ) -> serde_json::Value {
        RouteConfigService::write_configs_for_home(
            paths,
            pool,
            runtime,
            "http://127.0.0.1:43111",
            "claude",
            home,
        )
        .await
        .expect("write claude config");

        let written = tokio::fs::read_to_string(home.join(".claude/settings.json"))
            .await
            .expect("read settings");
        serde_json::from_str(&written).expect("valid json")
    }

    #[tokio::test]
    async fn a_fresh_claude_write_carries_the_credential_claude_code_authenticates_with() {
        // 0.7.0 shipped with only ANTHROPIC_BASE_URL here: the client was told
        // where the proxy is but not how to authenticate to it, so every request
        // died at 401 before a credential was picked — invisible in both the
        // request log and the usage stats. `write_existing_config_for_home` has its
        // own assertion; this pins the path behind the "write client config"
        // button, which is the one users actually take.
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;

        let token = json["env"]["ANTHROPIC_AUTH_TOKEN"]
            .as_str()
            .expect("ANTHROPIC_AUTH_TOKEN must be written");
        assert!(!token.is_empty());
        // Same value as the ledger entry, so the proxy recognizes the caller.
        assert_eq!(json["aiSwitch"]["routeProxy"]["apiKey"], token);
        // Two keys nothing ever read; one of them leaked the credential to every
        // process the agent spawns.
        assert!(json["env"].get("AI_SWITCH_ROUTE_PROXY").is_none());
        assert!(json["env"].get("AI_SWITCH_ROUTE_PROXY_API_KEY").is_none());
    }

    #[tokio::test]
    async fn subagent_alias_is_written_only_when_a_pool_account_configures_it() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");

        // No account configures a subagent mapping yet.
        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;
        assert!(json["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none());

        seed_claude_pool_member(
            &pool,
            "api",
            r#"[{"from":"claude-subagent","to":"provider-haiku"}]"#,
        )
        .await;

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;
        // The generic alias, NOT the account's upstream model: one settings file
        // serves the whole pool, so the proxy does the per-account translation.
        assert_eq!(json["env"]["CLAUDE_CODE_SUBAGENT_MODEL"], "claude-subagent");
        assert_ne!(json["env"]["CLAUDE_CODE_SUBAGENT_MODEL"], "provider-haiku");
    }

    #[tokio::test]
    async fn subagent_alias_is_cleared_when_no_account_configures_it() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        seed_claude_pool_member(
            &pool,
            "api",
            r#"[{"from":"claude-sonnet-alias","to":"provider-sonnet","label":"Sonnet"}]"#,
        )
        .await;

        let claude_dir = home.path().join(".claude");
        tokio::fs::create_dir_all(&claude_dir).await.expect("mkdir");
        tokio::fs::write(
            claude_dir.join("settings.json"),
            r#"{"env":{"CLAUDE_CODE_SUBAGENT_MODEL":"stale-alias","EXISTING_FLAG":"1"}}"#,
        )
        .await
        .expect("seed stale settings");

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;

        // Mirror-inverse: a stale value must never harden into an explicit setting.
        assert!(json["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none());
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
    }

    #[tokio::test]
    async fn subagent_alias_ignores_official_accounts() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        // Official credentials forward bodies without model rewriting, so an
        // alias would reach the vendor verbatim and 404.
        seed_claude_pool_member(
            &pool,
            "official",
            r#"[{"from":"claude-subagent","to":"provider-haiku"}]"#,
        )
        .await;

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;

        assert!(json["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none());
    }

    fn slot_mapping(alias: &str, to: &str, label: Option<&str>, one_m: bool) -> ModelMapping {
        ModelMapping {
            from: alias.to_string(),
            to: to.to_string(),
            label: label.map(str::to_string),
            supports_1m: one_m.then_some(true),
        }
    }

    fn sonnet_write(mappings: &[Vec<ModelMapping>]) -> ClaudeSlotWrite {
        RouteConfigService::resolve_claude_slot_writes(mappings)
            .into_iter()
            .next()
            .expect("sonnet slot")
    }

    #[test]
    fn unconfigured_slots_are_cleared() {
        let writes = RouteConfigService::resolve_claude_slot_writes(&[vec![slot_mapping(
            "claude-sonnet-alias",
            "provider-sonnet",
            None,
            false,
        )]]);

        assert_eq!(writes.len(), CLAUDE_MODEL_SLOTS.len());
        assert_eq!(writes[0].model.as_deref(), Some("claude-sonnet-alias"));
        // Opus/Fable/Haiku unconfigured → cleared, not defaulted to something.
        assert_eq!(writes[1], ClaudeSlotWrite::default());
        assert_eq!(writes[3], ClaudeSlotWrite::default());
    }

    #[test]
    fn slot_model_is_the_generic_alias_never_the_upstream_name() {
        let write = sonnet_write(&[vec![slot_mapping(
            "claude-sonnet-alias",
            "deepseek-v3-0324",
            None,
            false,
        )]]);

        assert_eq!(write.model.as_deref(), Some("claude-sonnet-alias"));
    }

    #[test]
    fn one_m_is_declared_only_when_every_account_supports_it() {
        let all = sonnet_write(&[
            vec![slot_mapping("claude-sonnet-alias", "a", None, true)],
            vec![slot_mapping("claude-sonnet-alias", "b", None, true)],
        ]);
        assert_eq!(all.model.as_deref(), Some("claude-sonnet-alias[1M]"));

        // AND, not OR: declaring 1M here would route a 1M-sized prompt to the
        // account that cannot serve it.
        let mixed = sonnet_write(&[
            vec![slot_mapping("claude-sonnet-alias", "a", None, true)],
            vec![slot_mapping("claude-sonnet-alias", "b", None, false)],
        ]);
        assert_eq!(mixed.model.as_deref(), Some("claude-sonnet-alias"));
    }

    #[test]
    fn one_m_ignores_accounts_that_do_not_configure_the_slot() {
        // The second account configures only Haiku, so it must not veto Sonnet's 1M.
        let write = sonnet_write(&[
            vec![slot_mapping("claude-sonnet-alias", "a", None, true)],
            vec![slot_mapping("claude-haiku-alias", "b", None, false)],
        ]);

        assert_eq!(write.model.as_deref(), Some("claude-sonnet-alias[1M]"));
    }

    #[test]
    fn display_name_needs_pool_consensus() {
        let agreed = sonnet_write(&[
            vec![slot_mapping(
                "claude-sonnet-alias",
                "a",
                Some("DeepSeek V3"),
                false,
            )],
            vec![slot_mapping(
                "claude-sonnet-alias",
                "b",
                Some("DeepSeek V3"),
                false,
            )],
        ]);
        assert_eq!(agreed.display_name.as_deref(), Some("DeepSeek V3"));

        // Disagreement omits the key rather than silently picking a winner.
        let conflicting = sonnet_write(&[
            vec![slot_mapping(
                "claude-sonnet-alias",
                "a",
                Some("DeepSeek V3"),
                false,
            )],
            vec![slot_mapping(
                "claude-sonnet-alias",
                "b",
                Some("Kimi K2"),
                false,
            )],
        ]);
        assert_eq!(conflicting.display_name, None);
    }

    #[test]
    fn blank_display_names_abstain_instead_of_vetoing() {
        let write = sonnet_write(&[
            vec![slot_mapping(
                "claude-sonnet-alias",
                "a",
                Some("DeepSeek V3"),
                false,
            )],
            vec![slot_mapping("claude-sonnet-alias", "b", None, false)],
            vec![slot_mapping("claude-sonnet-alias", "c", Some("  "), false)],
        ]);

        assert_eq!(write.display_name.as_deref(), Some("DeepSeek V3"));
    }

    #[tokio::test]
    async fn write_claude_config_pins_model_slots_and_display_names() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        seed_claude_pool_member(
            &pool,
            "api",
            r#"[{"from":"claude-sonnet-alias","to":"deepseek-v3","label":"DeepSeek V3","supports_1m":true},
                {"from":"claude-haiku-alias","to":"provider-haiku","label":"Haiku Fast"}]"#,
        )
        .await;

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;
        let env = &json["env"];

        // Generic aliases pin the client contract; the upstream name never leaks.
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            "claude-sonnet-alias[1M]"
        );
        assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"], "DeepSeek V3");
        assert_ne!(env["ANTHROPIC_DEFAULT_SONNET_MODEL"], "deepseek-v3");

        assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "claude-haiku-alias");
        assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"], "Haiku Fast");

        // Unconfigured slots stay absent rather than being invented.
        assert!(env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
        assert!(env.get("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME").is_none());
    }

    #[tokio::test]
    async fn stale_check_reports_pending_pool_and_client_config_edits() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        let base_url = "http://127.0.0.1:43111";
        let is_stale = || {
            RouteConfigService::config_write_is_stale_for_home(
                &paths,
                &pool,
                base_url,
                "claude",
                home.path(),
            )
        };

        // Never written for this platform → nothing to nudge about.
        assert!(!is_stale().await);

        seed_claude_pool_member(
            &pool,
            "api",
            r#"[{"from":"claude-sonnet-alias","to":"provider-sonnet"}]"#,
        )
        .await;
        write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;

        // Just written → in sync.
        assert!(!is_stale().await);

        // A mapping edit alone makes the on-disk file stale: the app writes only
        // on demand, so this is exactly the state the UI must surface.
        seed_claude_pool_member(
            &pool,
            "api",
            r#"[{"from":"claude-opus-alias","to":"provider-opus"}]"#,
        )
        .await;
        assert!(is_stale().await);

        write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;
        assert!(!is_stale().await);

        // Same for a global client-config edit, which never touches the pool.
        let mut settings = SettingsService::load(&paths).await.expect("settings");
        settings.claude_client_config_json = Some(r#"{"includeCoAuthoredBy":false}"#.to_string());
        SettingsService::save(&paths, &settings)
            .await
            .expect("save settings");
        assert!(is_stale().await);
    }

    #[tokio::test]
    async fn write_claude_config_covers_every_managed_model_key() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        // Every role configured, all 1M, one shared display name — the fully
        // populated shape a single-account pool produces.
        let mappings = CLAUDE_MODEL_SLOTS
            .iter()
            .map(|slot| slot.alias)
            .chain([CLAUDE_SUBAGENT_MODEL_ALIAS, FALLBACK_MODEL_ALIAS])
            .map(|alias| {
                format!(
                    r#"{{"from":"{alias}","to":"claude-opus-5","label":"claude-opus-5","supports_1m":true}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        seed_claude_pool_member(&pool, "api", &format!("[{mappings}]")).await;

        let json = write_claude_config_with_pool(home.path(), &paths, &pool, &runtime).await;
        let env = json["env"].as_object().expect("env object");

        for slot in CLAUDE_MODEL_SLOTS {
            assert_eq!(env[slot.model_env_key], format!("{}[1M]", slot.alias));
            assert_eq!(env[slot.name_env_key], "claude-opus-5");
        }
        // The subagent and the catch-all are aliases too, so they carry [1M] the
        // same way and get rewritten per account by the proxy.
        assert_eq!(env["CLAUDE_CODE_SUBAGENT_MODEL"], "claude-subagent[1M]");
        assert_eq!(env["ANTHROPIC_MODEL"], "claude-model[1M]");

        // Ten managed model keys in total: 4 slots x (model + display name),
        // plus the subagent and the catch-all.
        let managed = env
            .keys()
            .filter(|key| {
                key.starts_with("ANTHROPIC_DEFAULT_")
                    || key.as_str() == "ANTHROPIC_MODEL"
                    || key.as_str() == "CLAUDE_CODE_SUBAGENT_MODEL"
            })
            .count();
        assert_eq!(managed, 10);
    }

    #[test]
    fn malformed_client_config_is_ignored_rather_than_blocking_the_write() {
        // A bad value must not lock the user out of writing any config at all.
        for raw in [
            Some("not json"),
            Some("[1,2,3]"),
            Some("\"a string\""),
            Some("{}"),
            Some("   "),
            None,
        ] {
            assert_eq!(RouteConfigService::parse_claude_client_config(raw), None);
        }

        let parsed = RouteConfigService::parse_claude_client_config(Some(
            r#"{"includeCoAuthoredBy":false}"#,
        ))
        .expect("object");
        assert_eq!(parsed["includeCoAuthoredBy"], Value::Bool(false));
    }

    #[tokio::test]
    async fn write_existing_config_refuses_to_overwrite_invalid_user_config() {
        let (_app_dir, paths, pool, runtime) = config_write_context().await;
        let home = tempfile::tempdir().expect("home dir");
        let codex_dir = home.path().join(".codex");
        tokio::fs::create_dir_all(&codex_dir).await.expect("mkdir");
        let codex_path = codex_dir.join("config.toml");
        let original = "model_provider = [not valid TOML";
        tokio::fs::write(&codex_path, original)
            .await
            .expect("seed invalid config");

        let error = RouteConfigService::write_existing_config_for_home(
            &paths,
            &pool,
            &runtime,
            home.path(),
            "http://127.0.0.1:43111",
            "codex",
            "sk-ai-switch-test",
        )
        .await
        .expect_err("invalid existing config must not be overwritten");

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "filesystem.route_config_write",
                details: Some(details),
                ..
            } if details.contains("validation.route_config_existing_invalid")
        ));
        assert_eq!(
            tokio::fs::read_to_string(&codex_path)
                .await
                .expect("read original"),
            original
        );
    }

    #[tokio::test]
    async fn write_existing_configs_for_home_fails_before_overwriting_invalid_config() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let home = tempfile::tempdir().expect("home dir");
        let codex_dir = home.path().join(".codex");
        tokio::fs::create_dir_all(&codex_dir).await.expect("mkdir");
        let codex_path = codex_dir.join("config.toml");
        let original = "model_provider = [not valid TOML";
        tokio::fs::write(&codex_path, original)
            .await
            .expect("seed invalid config");

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let app_dir = tempfile::tempdir().expect("app dir");
        let paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let runtime = ConfigWriteRuntimeState::default();
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex key");

        let error = RouteConfigService::write_existing_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect_err("invalid config must fail the batch before writes");

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "filesystem.route_config_write",
                details: Some(details),
                ..
            } if details.contains("validation.route_config_existing_invalid")
        ));
        assert_eq!(
            tokio::fs::read_to_string(&codex_path)
                .await
                .expect("read original"),
            original
        );
    }

    #[tokio::test]
    async fn write_existing_configs_for_home_does_not_partially_update_another_platform() {
        use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;

        let home = tempfile::tempdir().expect("home dir");
        let claude_dir = home.path().join(".claude");
        let grok_dir = home.path().join(".grok");
        tokio::fs::create_dir_all(&claude_dir)
            .await
            .expect("claude mkdir");
        tokio::fs::create_dir_all(&grok_dir)
            .await
            .expect("grok mkdir");
        let claude_path = claude_dir.join("settings.json");
        let grok_path = grok_dir.join("settings.json");
        let claude_original = r#"{"env":{"EXISTING_FLAG":"1"}}"#;
        let grok_original = "{not valid JSON";
        tokio::fs::write(&claude_path, claude_original)
            .await
            .expect("seed claude");
        tokio::fs::write(&grok_path, grok_original)
            .await
            .expect("seed grok");

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let app_dir = tempfile::tempdir().expect("app dir");
        let paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let runtime = ConfigWriteRuntimeState::default();
        RouteProxyKeyRepository::ensure_platform_key(&pool, "claude", "sk-claude")
            .await
            .expect("claude key");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-grok")
            .await
            .expect("grok key");

        let error = RouteConfigService::write_existing_configs_for_home(
            &paths,
            &pool,
            &runtime,
            "http://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect_err("invalid Grok config must prevent all writes");

        assert!(matches!(
            error,
            AppError::Filesystem {
                code: "filesystem.route_config_write",
                details: Some(details),
                ..
            } if details.contains("validation.route_config_existing_invalid")
        ));
        assert_eq!(
            tokio::fs::read_to_string(&claude_path)
                .await
                .expect("read claude"),
            claude_original
        );
        assert_eq!(
            tokio::fs::read_to_string(&grok_path)
                .await
                .expect("read grok"),
            grok_original
        );
    }

    async fn config_write_context() -> (
        tempfile::TempDir,
        AppPaths,
        sqlx::SqlitePool,
        ConfigWriteRuntimeState,
    ) {
        let app_dir = tempfile::tempdir().expect("app dir");
        let paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        (app_dir, paths, pool, ConfigWriteRuntimeState::default())
    }
}
