use crate::config_writer::ConfigWriter;
use crate::database::repositories::route_proxy_key_repository::RouteProxyKeyRepository;
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::services::route_pool_service::normalize_platform;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use toml_edit::{value, Document, Item, Table};
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

struct RouteConfigWritePlan {
    target_key: String,
    path: PathBuf,
    route_proxy_key: String,
    content: String,
    before_content: Option<String>,
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

        Self::write_configs_for_home(paths, pool, base_url, platform, &home).await
    }

    pub(crate) async fn write_configs_for_home(
        _paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
        platform: &str,
        home: &Path,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        // Keep a local backup root available for later snapshot wiring.
        let target_key = normalize_platform(platform)?;
        // Stable per-platform local key so the shared proxy can resolve agent pools by API key.
        let existing_route_proxy_key =
            RouteProxyKeyRepository::get_existing_platform_key(pool, &target_key).await?;
        let route_proxy_key = RouteProxyKeyRepository::ensure_platform_key(
            pool,
            &target_key,
            &generate_route_proxy_key(),
        )
        .await?;
        match Self::write_existing_config_for_home(&home, base_url, &target_key, &route_proxy_key)
            .await
        {
            Ok(outcome) => Ok(vec![outcome]),
            Err(error) => {
                if existing_route_proxy_key.is_none() {
                    let _ = RouteProxyKeyRepository::delete_if_matches(
                        pool,
                        &target_key,
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
        _paths: &AppPaths,
        pool: &SqlitePool,
        base_url: &str,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let home = resolve_home_dir()?;
        Self::write_existing_configs_for_home(pool, base_url, &home).await
    }

    pub(crate) async fn write_existing_configs_for_home(
        pool: &SqlitePool,
        base_url: &str,
        home: &Path,
    ) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
        let base_url = normalize_base_url(base_url)?;
        let platforms = RouteProxyKeyRepository::list_platforms(pool).await?;
        let mut plans = Vec::with_capacity(platforms.len());
        let mut skipped = Vec::new();

        for platform in platforms {
            match RouteProxyKeyRepository::get_existing_platform_key(pool, &platform).await? {
                Some(route_proxy_key) => {
                    // Validate every managed config before changing any client file.
                    plans.push(
                        prepare_route_config_for_home(home, base_url, &platform, &route_proxy_key)
                            .await?,
                    );
                }
                None => skipped.push(RouteConfigWriteOutcome {
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

        let mut outcomes = write_route_config_plans(&plans).await?;
        outcomes.extend(skipped);
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
        let plan =
            prepare_route_config_for_home(home, base_url, &target_key, route_proxy_key).await?;
        write_route_config_plan(&plan).await
    }
}

async fn prepare_route_config_for_home(
    home: &Path,
    base_url: &str,
    platform: &str,
    route_proxy_key: &str,
) -> Result<RouteConfigWritePlan, AppError> {
    let target_key = normalize_platform(platform)?;
    let target = route_config_target(home, &target_key)?;
    let (before_content, content) =
        merge_route_config_content(&target, base_url, route_proxy_key).await?;

    Ok(RouteConfigWritePlan {
        target_key: target.key.to_string(),
        path: target.path,
        route_proxy_key: route_proxy_key.to_string(),
        content,
        before_content,
    })
}

async fn merge_route_config_content(
    target: &RouteConfigTarget,
    base_url: &str,
    route_proxy_key: &str,
) -> Result<(Option<String>, String), AppError> {
    let existing = match tokio::fs::read_to_string(&target.path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((None, (target.render)(base_url, route_proxy_key)));
        }
        Err(error) => return Err(error.into()),
    };

    if existing.trim().is_empty() {
        return Ok((Some(existing), (target.render)(base_url, route_proxy_key)));
    }

    let content = match target.key {
        "codex" => merge_codex_config(&target.path, &existing, base_url, route_proxy_key),
        "claude" | "gemini" | "grok" => merge_json_agent_config(
            &target.path,
            &existing,
            target.key,
            base_url,
            route_proxy_key,
        ),
        _ => Ok((target.render)(base_url, route_proxy_key)),
    }?;
    Ok((Some(existing), content))
}

async fn write_route_config_plan(
    plan: &RouteConfigWritePlan,
) -> Result<RouteConfigWriteOutcome, AppError> {
    let write = ConfigWriter::write_atomic(&plan.path, &plan.content).await?;
    Ok(RouteConfigWriteOutcome {
        target_key: plan.target_key.clone(),
        path: write.path,
        status: write.status,
        route_proxy_key: plan.route_proxy_key.clone(),
        error: None,
    })
}

async fn write_route_config_plans(
    plans: &[RouteConfigWritePlan],
) -> Result<Vec<RouteConfigWriteOutcome>, AppError> {
    let mut outcomes = Vec::with_capacity(plans.len());
    let mut written = Vec::with_capacity(plans.len());

    for plan in plans {
        match write_route_config_plan(plan).await {
            Ok(outcome) => {
                written.push(plan);
                outcomes.push(outcome);
            }
            Err(error) => {
                let rollback_errors = rollback_route_config_plans(&written).await;
                return Err(route_config_write_error(error, rollback_errors));
            }
        }
    }

    Ok(outcomes)
}

async fn rollback_route_config_plans(plans: &[&RouteConfigWritePlan]) -> Vec<String> {
    let mut errors = Vec::new();
    for plan in plans.iter().rev() {
        let result = match &plan.before_content {
            Some(content) => ConfigWriter::write_atomic(&plan.path, content)
                .await
                .map(|_| ()),
            None => match tokio::fs::remove_file(&plan.path).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            },
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", plan.path.display()));
        }
    }
    errors
}

fn route_config_write_error(write_error: AppError, rollback_errors: Vec<String>) -> AppError {
    let details = if rollback_errors.is_empty() {
        format!("{write_error}; prior client config writes were restored")
    } else {
        format!(
            "{write_error}; rollback failed for {}",
            rollback_errors.join(" | ")
        )
    };
    AppError::Filesystem {
        code: "filesystem.route_config_write",
        message: "Could not write route proxy configuration".to_string(),
        details: Some(details),
        recoverable: true,
    }
}

fn merge_codex_config(
    path: &Path,
    existing: &str,
    base_url: &str,
    route_proxy_key: &str,
) -> Result<String, AppError> {
    let base_url = codex_route_proxy_base_url(base_url);
    let mut document = existing
        .parse::<Document>()
        .map_err(|error| invalid_existing_config(path, "TOML", error.to_string()))?;

    document["model_provider"] = value("ai-switch");
    if document.get("model_providers").is_none() {
        document["model_providers"] = Item::Table(Table::new());
    }
    let providers = document["model_providers"].as_table_mut().ok_or_else(|| {
        invalid_existing_config(
            path,
            "TOML",
            "model_providers must be a table to add the ai-switch provider".to_string(),
        )
    })?;
    if !providers.contains_key("ai-switch") {
        providers.insert("ai-switch", Item::Table(Table::new()));
    }
    let provider = providers["ai-switch"].as_table_mut().ok_or_else(|| {
        invalid_existing_config(
            path,
            "TOML",
            "model_providers.ai-switch must be a table".to_string(),
        )
    })?;
    provider["name"] = value("AI Switch Route Proxy");
    provider["base_url"] = value(base_url);
    provider["wire_api"] = value("responses");
    provider["api_key"] = value(route_proxy_key);

    Ok(document.to_string())
}

fn merge_json_agent_config(
    path: &Path,
    existing: &str,
    platform: &str,
    base_url: &str,
    route_proxy_key: &str,
) -> Result<String, AppError> {
    let mut config: Value = serde_json::from_str(existing)
        .map_err(|error| invalid_existing_config(path, "JSON", error.to_string()))?;
    let root = config.as_object_mut().ok_or_else(|| {
        invalid_existing_config(path, "JSON", "root value must be an object".to_string())
    })?;

    let ai_switch = object_entry(root, "aiSwitch", path, "JSON")?;
    let route_proxy = object_entry(ai_switch, "routeProxy", path, "JSON")?;
    route_proxy.insert("enabled".to_string(), Value::Bool(true));
    route_proxy.insert("baseUrl".to_string(), Value::String(base_url.to_string()));
    route_proxy.insert("platform".to_string(), Value::String(platform.to_string()));
    route_proxy.insert(
        "apiKey".to_string(),
        Value::String(route_proxy_key.to_string()),
    );

    let env = object_entry(root, "env", path, "JSON")?;
    match platform {
        "claude" => {
            env.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(base_url.to_string()),
            );
        }
        "gemini" => {
            env.insert(
                "GEMINI_API_BASE_URL".to_string(),
                Value::String(base_url.to_string()),
            );
            env.insert(
                "GOOGLE_GEMINI_BASE_URL".to_string(),
                Value::String(base_url.to_string()),
            );
        }
        "grok" => {
            env.insert(
                "XAI_API_BASE_URL".to_string(),
                Value::String(base_url.to_string()),
            );
            env.insert(
                "GROK_API_BASE_URL".to_string(),
                Value::String(base_url.to_string()),
            );
        }
        _ => {}
    }
    env.insert(
        "AI_SWITCH_ROUTE_PROXY".to_string(),
        Value::String(base_url.to_string()),
    );
    env.insert(
        "AI_SWITCH_ROUTE_PROXY_API_KEY".to_string(),
        Value::String(route_proxy_key.to_string()),
    );

    serde_json::to_string_pretty(&config).map_err(AppError::from)
}

fn object_entry<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
    path: &Path,
    format: &str,
) -> Result<&'a mut Map<String, Value>, AppError> {
    let value = parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    value
        .as_object_mut()
        .ok_or_else(|| invalid_existing_config(path, format, format!("{key} must be an object")))
}

fn invalid_existing_config(path: &Path, format: &str, details: String) -> AppError {
    AppError::Validation {
        code: "validation.route_config_existing_invalid",
        message: "Existing CLI configuration is invalid; refusing to overwrite it".to_string(),
        details: Some(format!("{} ({format}): {details}", path.display())),
        recoverable: true,
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
    let base_url = codex_route_proxy_base_url(base_url);
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

fn codex_route_proxy_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if base_last_path_segment(trimmed).is_some_and(|segment| segment.eq_ignore_ascii_case("v1")) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn base_last_path_segment(base_url: &str) -> Option<&str> {
    let after_scheme = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let path = after_scheme.split_once('/').map(|(_, path)| path)?;
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
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
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111/v1\""));
    }

    #[test]
    fn render_codex_config_keeps_existing_v1_suffix() {
        let rendered = render_codex_config("http://127.0.0.1:43111/v1/", "sk-ai-switch-test");
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111/v1\""));
        assert!(!rendered.contains("/v1/v1"));
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
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111/v1\""));
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
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = RouteConfigService::write_configs_for_home(
            &paths,
            &pool,
            "http://127.0.0.1:43111",
            "codex",
            home.path(),
        )
        .await
        .expect_err("invalid config must fail");

        assert!(error.to_string().contains("refusing to overwrite"));
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
        let _paths = AppPaths::from_data_dir(app_dir.path().to_path_buf());
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex key");

        let outcomes = RouteConfigService::write_existing_configs_for_home(
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

    #[tokio::test]
    async fn write_existing_codex_config_preserves_unmanaged_toml() {
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
        assert!(written.contains("api_key = \"sk-ai-switch-test\""));
    }

    #[tokio::test]
    async fn write_existing_json_config_preserves_unmanaged_settings_and_env() {
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
        let home = tempfile::tempdir().expect("home dir");
        let codex_dir = home.path().join(".codex");
        tokio::fs::create_dir_all(&codex_dir).await.expect("mkdir");
        let codex_path = codex_dir.join("config.toml");
        let original = "model_provider = [not valid TOML";
        tokio::fs::write(&codex_path, original)
            .await
            .expect("seed invalid config");

        let error = RouteConfigService::write_existing_config_for_home(
            home.path(),
            "http://127.0.0.1:43111",
            "codex",
            "sk-ai-switch-test",
        )
        .await
        .expect_err("invalid existing config must not be overwritten");

        match error {
            AppError::Validation { code, .. } => {
                assert_eq!(code, "validation.route_config_existing_invalid");
            }
            other => panic!("expected validation error, got {other:?}"),
        }
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
        RouteProxyKeyRepository::ensure_platform_key(&pool, "codex", "sk-codex")
            .await
            .expect("codex key");

        let error = RouteConfigService::write_existing_configs_for_home(
            &pool,
            "http://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect_err("invalid config must fail the batch before writes");

        assert!(error.to_string().contains("refusing to overwrite"));
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
        RouteProxyKeyRepository::ensure_platform_key(&pool, "claude", "sk-claude")
            .await
            .expect("claude key");
        RouteProxyKeyRepository::ensure_platform_key(&pool, "grok", "sk-grok")
            .await
            .expect("grok key");

        let error = RouteConfigService::write_existing_configs_for_home(
            &pool,
            "http://127.0.0.1:43111",
            home.path(),
        )
        .await
        .expect_err("invalid Grok config must prevent all writes");

        assert!(error.to_string().contains("refusing to overwrite"));
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
}
