use crate::config_writer::ConfigWriter;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::services::route_pool_service::normalize_platform;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteConfigWriteOutcome {
    pub target_key: String,
    pub path: String,
    pub status: String,
    pub route_proxy_key: String,
    pub error: Option<String>,
}

type TargetRender = fn(&str, &str) -> String;

struct RouteConfigTarget {
    key: &'static str,
    path: PathBuf,
    render: TargetRender,
}

pub struct RouteConfigService;

impl RouteConfigService {
    pub async fn write_configs(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;

        let home = BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| AppError::Filesystem {
                code: "filesystem.home_not_found",
                message: "Could not resolve the current user home directory".to_string(),
                details: None,
                recoverable: false,
            })?;

        // Keep a local backup root available for later snapshot wiring.
        let _ = paths;

        let target_key = normalize_platform(platform)?;
        // Stable per-platform local key so the shared proxy can resolve agent pools by API key.
        let route_proxy_key = RouteProxyKeyRepository::ensure_platform_key(
            pool,
            &target_key,
            &generate_route_proxy_key(),
        )
        .await?;
        Ok(vec![
            Self::write_existing_config_for_home(&home, base_url, &target_key, &route_proxy_key)
                .await?,
        ])
    }

    /// Rewrites only platforms that already own a managed proxy key. This is
    /// used for HTTP/HTTPS changes and never creates additional client config.
    pub async fn write_existing_configs(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let home = resolve_home_dir()?;
        Self::write_existing_configs_for_home(paths, pool, base_url, &home).await
    }

    pub(crate) async fn write_existing_configs_for_home(
        paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        home: &Path,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platforms = RouteProxyKeyRepository::list_platforms(pool).await?;
        let mut outcomes = Vec::with_capacity(platforms.len());

        for platform in platforms {
            match RouteProxyKeyRepository::get_existing_platform_key(pool, &platform).await? {
                Some(route_proxy_key) => {
                    match Self::write_existing_config_for_home(
                        home,
                        base_url,
                        &platform,
                        &route_proxy_key,
                    )
                    .await
                    {
                        Ok(outcome) => outcomes.push(outcome),
                        Err(error) => outcomes.push(RouteConfigWriteOutcome {
                            target_key: platform,
                            path: String::new(),
                            status: "error".to_string(),
                            route_proxy_key,
                            error: Some(error.to_string()),
                        }),
                    }
                }
                None => outcomes.push(RouteConfigWriteOutcome {
                    target_key: platform,
                    path: String::new(),
                    status: "skipped".to_string(),
                    route_proxy_key: String::new(),
                    error: Some(
                        "Route proxy key was removed before HTTPS config rewrite".to_string(),
                    ),
                }),
            }
        }

        Ok(outcomes)
    }

    pub(crate) async fn write_existing_config_for_home(
        home: &Path,
        base_url: &str,
        platform: &str,
        route_proxy_key: &str,
    ) -> Result<RouteConfigWriteOutcome, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let target_key = normalize_platform(platform)?;
        let target = route_config_target(home, &target_key)?;
        let content = (target.render)(base_url, route_proxy_key);
        let write = ConfigWriter::write_atomic(&target.path, &content).await?;

        Ok(RouteConfigWriteOutcome {
            target_key: target.key.to_string(),
            path: write.path,
            status: write.status,
            route_proxy_key: route_proxy_key.to_string(),
            error: None,
        })
    }
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

fn route_config_target(home: &Path, target_key: &str) -> Result<RouteConfigTarget, AppError> {
    match target_key {
        "codex" => Ok(RouteConfigTarget {
            key: "codex",
            path: home.join(".codex").join("config.toml"),
            render: render_codex_config,
        }),
        "claude" => Ok(RouteConfigTarget {
            key: "claude",
            path: home.join(".claude").join("settings.json"),
            render: render_claude_config,
        }),
        "gemini" => Ok(RouteConfigTarget {
            key: "gemini",
            path: home.join(".gemini").join("settings.json"),
            render: render_gemini_config,
        }),
        "grok" => Ok(RouteConfigTarget {
            key: "grok",
            path: home.join(".grok").join("settings.json"),
            render: render_grok_config,
        }),
        other => Err(AppError::Validation {
            code: "validation.route_config_target_unsupported",
            message: "Route config writing is not supported for this target".to_string(),
            details: Some(other.to_string()),
            recoverable: true,
        }),
    }
}

pub fn generate_route_proxy_key() -> String {
    format!("sk-ai-switch-{}", Uuid::new_v4().simple())
}

pub fn render_codex_config(base_url: &str, route_proxy_key: &str) -> String {
    format!(
        r#"# Generated by AI Switch route proxy
model_provider = "ai-switch"

[model_providers.ai-switch]
name = "AI Switch Route Proxy"
base_url = "{base_url}"
wire_api = "responses"
api_key = "{route_proxy_key}"
"#
    )
}

pub fn render_claude_config(base_url: &str, route_proxy_key: &str) -> String {
    serde_json::json!({
        "aiSwitch": {
            "routeProxy": {
                "enabled": true,
                "baseUrl": base_url,
                "platform": "claude",
                "apiKey": route_proxy_key
            }
        },
        "env": {
            "ANTHROPIC_BASE_URL": base_url,
            "AI_SWITCH_ROUTE_PROXY": base_url,
            "AI_SWITCH_ROUTE_PROXY_API_KEY": route_proxy_key
        }
    })
    .to_string()
}

pub fn render_gemini_config(base_url: &str, route_proxy_key: &str) -> String {
    serde_json::json!({
        "aiSwitch": {
            "routeProxy": {
                "enabled": true,
                "baseUrl": base_url,
                "platform": "gemini",
                "apiKey": route_proxy_key
            }
        },
        "env": {
            "GEMINI_API_BASE_URL": base_url,
            "GOOGLE_GEMINI_BASE_URL": base_url,
            "AI_SWITCH_ROUTE_PROXY": base_url,
            "AI_SWITCH_ROUTE_PROXY_API_KEY": route_proxy_key
        }
    })
    .to_string()
}

pub fn render_grok_config(base_url: &str, route_proxy_key: &str) -> String {
    serde_json::json!({
        "aiSwitch": {
            "routeProxy": {
                "enabled": true,
                "baseUrl": base_url,
                "platform": "grok",
                "apiKey": route_proxy_key
            }
        },
        "env": {
            "XAI_API_BASE_URL": base_url,
            "GROK_API_BASE_URL": base_url,
            "AI_SWITCH_ROUTE_PROXY": base_url,
            "AI_SWITCH_ROUTE_PROXY_API_KEY": route_proxy_key
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[test]
    fn render_codex_config_points_model_provider_to_proxy() {
        let rendered = render_codex_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        assert!(rendered.contains("model_provider = \"ai-switch\""));
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111\""));
    }

    #[test]
    fn render_grok_includes_route_metadata() {
        let grok = render_grok_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        assert!(grok.contains("XAI_API_BASE_URL"));
        assert!(grok.contains("\"platform\":\"grok\""));
        assert!(grok.contains("\"apiKey\":\"sk-ai-switch-test\""));
    }

    #[test]
    fn render_claude_and_gemini_include_route_metadata() {
        let claude = render_claude_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        let gemini = render_gemini_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        assert!(claude.contains("ANTHROPIC_BASE_URL"));
        assert!(claude.contains("\"platform\":\"claude\""));
        assert!(gemini.contains("GEMINI_API_BASE_URL"));
        assert!(gemini.contains("\"platform\":\"gemini\""));
    }

    #[test]
    fn generated_route_proxy_key_uses_sk_shape() {
        let key = generate_route_proxy_key();
        assert!(key.starts_with("sk-ai-switch-"));
        assert!(key.len() > "sk-ai-switch-".len() + 20);
    }

    #[test]
    fn render_codex_config_uses_responses_and_route_proxy_key() {
        let rendered = render_codex_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        assert!(rendered.contains("model_provider = \"ai-switch\""));
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111\""));
        assert!(rendered.contains("wire_api = \"responses\""));
        assert!(rendered.contains("api_key = \"sk-ai-switch-test\""));
        assert!(!rendered.contains("wire_api = \"chat\""));
    }

    #[test]
    fn render_claude_and_gemini_include_route_proxy_key_metadata() {
        let claude = render_claude_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        let gemini = render_gemini_config("http://127.0.0.1:43111", "sk-ai-switch-test");
        assert!(claude.contains("\"apiKey\":\"sk-ai-switch-test\""));
        assert!(claude.contains("AI_SWITCH_ROUTE_PROXY_API_KEY"));
        assert!(gemini.contains("\"apiKey\":\"sk-ai-switch-test\""));
        assert!(gemini.contains("AI_SWITCH_ROUTE_PROXY_API_KEY"));
    }

    #[tokio::test]
    async fn write_configs_rejects_unsupported_platform_without_writing_all_targets() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::from_data_dir(temp.path().to_path_buf());
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let error =
            RouteConfigService::write_configs(&paths, &pool, "http://127.0.0.1:43111", "opencode")
                .await
                .expect_err("unsupported target");

        match error {
            AppError::Validation { code, details, .. } => {
                assert_eq!(code, "validation.route_config_target_unsupported");
                assert_eq!(details.as_deref(), Some("opencode"));
            }
            other => panic!("expected validation error, got {other:?}"),
        }
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
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex key");

        let outcomes = RouteConfigService::write_existing_configs_for_home(
            &paths,
            &pool,
            "https://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect("write existing configs");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].target_key, "codex");
        assert_eq!(outcomes[0].route_proxy_key, "sk-codex");
        let codex_config = tokio::fs::read_to_string(home.path().join(".codex/config.toml"))
            .await
            .expect("codex config");
        assert!(codex_config.contains("https://127.0.0.1:43111"));
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
}
