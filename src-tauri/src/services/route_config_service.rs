use crate::adapters::route_config::{
    codex_model_catalog_path, RouteConfigInput, TargetAdapter, TargetAdapterRegistry,
};
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
use crate::models::config_snapshot::ConfigWriteOutcome;
use crate::models::platform::{PlatformId, PlatformOperation};
use crate::models::route_credential::{RouteCredentialPoolScope, CLAUDE_SUBAGENT_MODEL_ALIAS};
use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
use crate::paths::AppPaths;
use crate::services::config_write_service::{
    ConfigWriteCoordinator, ConfigWriteRequest, ConfigWriteRuntimeState,
};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_model_capability::{
    codex_model_catalog_payload, parse_model_capability,
};
use directories::BaseDirs;
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
        let subagent_model = Self::resolve_subagent_model(pool, platform).await?;
        let request = ConfigWriteRequest {
            adapter,
            home: home.to_path_buf(),
            input: RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key: route_proxy_key.clone(),
                subagent_model,
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
            let subagent_model = Self::resolve_subagent_model(pool, parsed).await?;
            requests.push(ConfigWriteRequest {
                adapter,
                home: home.to_path_buf(),
                input: RouteConfigInput {
                    base_url: base_url.to_string(),
                    route_proxy_key,
                    subagent_model,
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
        let subagent_model = Self::resolve_subagent_model(pool, platform).await?;
        let request = ConfigWriteRequest {
            adapter: route_config_adapter(platform)?,
            home: home.to_path_buf(),
            input: RouteConfigInput {
                base_url: base_url.to_string(),
                route_proxy_key: route_proxy_key.to_string(),
                subagent_model,
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

    /// Resolves the alias to write into the agent's subagent-model env key, or
    /// `None` when no enabled in-pool account configures one (which clears it).
    ///
    /// Always the generic alias, never an account's upstream model name: one
    /// settings file serves the whole pool, so the proxy must do the per-account
    /// translation. Official credentials are excluded — their bodies are
    /// forwarded without model rewriting, so an alias would reach the vendor
    /// verbatim and 404. Like the Codex catalog, this does not filter on
    /// `status`, so a cooling account still counts.
    async fn resolve_subagent_model(
        pool: &SqlitePool,
        platform: PlatformId,
    ) -> Result<Option<String>, AppError> {
        if platform != PlatformId::Claude {
            return Ok(None);
        }

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

        let configured = credentials
            .iter()
            .filter(|credential| credential.kind == "api")
            .any(|credential| {
                parse_model_capability(&credential.config_json)
                    .mappings
                    .iter()
                    .any(|mapping| {
                        mapping.from.trim() == CLAUDE_SUBAGENT_MODEL_ALIAS
                            && !mapping.to.trim().is_empty()
                    })
            });

        Ok(configured.then(|| CLAUDE_SUBAGENT_MODEL_ALIAS.to_string()))
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
        assert_eq!(
            json["env"]["AI_SWITCH_ROUTE_PROXY_API_KEY"],
            "sk-ai-switch-test"
        );
        assert_eq!(
            json["aiSwitch"]["routeProxy"]["baseUrl"],
            "https://127.0.0.1:43111"
        );
        assert_eq!(json["aiSwitch"]["routeProxy"]["platform"], "claude");
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
