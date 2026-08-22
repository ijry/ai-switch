# AI Switch Provider Switching B2.1 Codex Real Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Add explicit real provider switching for the Codex target by writing documented Codex user config through the existing atomic writer and snapshot pipeline.

**Architecture:** B2.1 extends the B1 provider switching service with a `real` mode branch for Codex only. A new Codex adapter resolves `CODEX_HOME` or `~/.codex/config.toml`, renders TOML without raw secrets, and preserves unrelated TOML data through parse/serialize. The frontend exposes a separate real Codex action while keeping sandbox switching unchanged.

**Tech Stack:** Tauri 2, React 18, TypeScript, Vite, Tailwind CSS, Vitest, Testing Library, Rust, sqlx SQLite, serde, serde_json, toml_edit, chrono, uuid, tokio.

## Global Constraints

- B2.1 must keep existing sandbox switching behavior unchanged.
- B2.1 must accept `mode = "real"` only when the target key is `codex`.
- B2.1 must not write real configs for Claude Code, Claude Desktop, Gemini CLI, OpenCode, OpenClaw, or Hermes.
- B2.1 must resolve real Codex config paths in the backend only.
- B2.1 must write Codex config through `ConfigWriter`.
- B2.1 must not write raw API keys or resolved secrets into Codex config.
- B2.1 must record successful real writes as `config_snapshots.operation = "switch_provider:real"`.
- B2.1 must record failed real attempts after path resolution when possible.
- B2.1 must preserve unrelated TOML keys and provider blocks after parse/serialize, but does not guarantee comment preservation.
- Clean-room rule: public behavior, public documentation, and public file formats may be studied, but non-commercial source code from `cockpit-tools` must not be copied or translated.

---

## File Structure

Create these backend files:

```text
src-tauri/src/adapters/codex_config.rs
```

Modify these backend files:

```text
src-tauri/Cargo.toml
src-tauri/src/adapters/mod.rs
src-tauri/src/services/provider_switch_service.rs
```

Modify these frontend files:

```text
src/lib/api/types.ts
src/screens/ProvidersScreen.tsx
tests/apiClient.test.ts
tests/ProvidersScreen.test.tsx
```

Modify docs:

```text
README.md
```

---

### Task 1: Add Codex Config Adapter

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/adapters/codex_config.rs`
- Modify: `src-tauri/src/adapters/mod.rs`

**Interfaces:**
- Consumes: `Provider`, existing `AppError`.
- Produces: `CodexRenderedConfig { path: PathBuf, contents: String, provider_slug: String }`.
- Produces: `resolve_codex_config_path() -> Result<PathBuf, AppError>`.
- Produces: `resolve_codex_config_path_with(codex_home: Option<&Path>, home_dir: &Path) -> Result<PathBuf, AppError>`.
- Produces: `render_codex_provider_config(path: &Path, provider: &Provider) -> Result<CodexRenderedConfig, AppError>`.
- Produces: `render_codex_provider_config_from_str(path: &Path, existing: &str, provider: &Provider) -> Result<CodexRenderedConfig, AppError>`.

- [x] **Step 1: Add TOML dependency**

Add this dependency to `src-tauri/Cargo.toml` under `[dependencies]`:

```toml
toml_edit = "0.22"
```

- [x] **Step 2: Write failing adapter tests**

Create `src-tauri/src/adapters/codex_config.rs` with this test-first scaffold:

```rust
use crate::error::AppError;
use crate::models::provider::Provider;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRenderedConfig {
    pub path: PathBuf,
    pub contents: String,
    pub provider_slug: String,
}

pub fn resolve_codex_config_path() -> Result<PathBuf, AppError> {
    unimplemented!("implemented in Step 4")
}

pub fn resolve_codex_config_path_with(
    _codex_home: Option<&Path>,
    _home_dir: &Path,
) -> Result<PathBuf, AppError> {
    unimplemented!("implemented in Step 4")
}

pub async fn render_codex_provider_config(
    _path: &Path,
    _provider: &Provider,
) -> Result<CodexRenderedConfig, AppError> {
    unimplemented!("implemented in Step 4")
}

pub fn render_codex_provider_config_from_str(
    _path: &Path,
    _existing: &str,
    _provider: &Provider,
) -> Result<CodexRenderedConfig, AppError> {
    unimplemented!("implemented in Step 4")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use toml_edit::DocumentMut;

    fn provider() -> Provider {
        Provider {
            id: "Provider-1".to_string(),
            name: "Acme Provider".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{}".to_string(),
            target_options_json: "{\"codex\":{\"env_key\":\"ACME_API_KEY\"}}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_codex_home_config_path() {
        let dir = tempdir().expect("tempdir");

        let path = resolve_codex_config_path_with(Some(dir.path()), Path::new("C:/Users/example"))
            .expect("path");

        assert_eq!(path, dir.path().join("config.toml"));
    }

    #[test]
    fn renders_provider_config_and_preserves_unrelated_toml() {
        let existing = r#"
model = "gpt-5.4"

[model_providers.other]
name = "Other"
base_url = "https://other.example.com/v1"
"#;

        let rendered = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            existing,
            &provider(),
        )
        .expect("rendered");
        let parsed = rendered.contents.parse::<DocumentMut>().expect("toml");

        assert_eq!(rendered.provider_slug, "ai_switch_provider_1");
        assert_eq!(parsed["model"].as_str(), Some("gpt-5.4"));
        assert_eq!(parsed["model_provider"].as_str(), Some("ai_switch_provider_1"));
        assert_eq!(
            parsed["model_providers"]["other"]["name"].as_str(),
            Some("Other")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["base_url"].as_str(),
            Some("https://api.example.com/v1")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["env_key"].as_str(),
            Some("ACME_API_KEY")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["wire_api"].as_str(),
            Some("responses")
        );
    }

    #[test]
    fn rejects_malformed_existing_toml() {
        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "model = ",
            &provider(),
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.codex_config_toml");
    }

    #[test]
    fn rejects_missing_base_url() {
        let mut provider = provider();
        provider.base_url = None;

        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_base_url_required");
    }

    #[test]
    fn rejects_malformed_target_options_json() {
        let mut provider = provider();
        provider.target_options_json = "{".to_string();

        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_target_options_json");
    }
}
```

Update `src-tauri/src/adapters/mod.rs`:

```rust
#![allow(dead_code)]

pub mod codex_config;
pub mod provider_renderers;
```

Keep the rest of `src-tauri/src/adapters/mod.rs` unchanged.

Run:

```powershell
pnpm rust:test codex_config
```

Expected: FAIL because the adapter functions are not implemented.

- [x] **Step 3: Implement Codex adapter**

Replace `src-tauri/src/adapters/codex_config.rs` with:

```rust
use crate::error::AppError;
use crate::models::provider::Provider;
use directories::BaseDirs;
use serde_json::Value as JsonValue;
use std::env;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRenderedConfig {
    pub path: PathBuf,
    pub contents: String,
    pub provider_slug: String,
}

pub fn resolve_codex_config_path() -> Result<PathBuf, AppError> {
    let home_dir = BaseDirs::new()
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?
        .home_dir()
        .to_path_buf();
    let codex_home = env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    resolve_codex_config_path_with(codex_home.as_deref(), &home_dir)
}

pub fn resolve_codex_config_path_with(
    codex_home: Option<&Path>,
    home_dir: &Path,
) -> Result<PathBuf, AppError> {
    let root = codex_home
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home_dir.join(".codex"));

    if root.as_os_str().is_empty() || !root.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.codex_config_path_invalid",
            message: "Codex config directory must be an absolute path".to_string(),
            details: Some(root.display().to_string()),
            recoverable: false,
        });
    }

    Ok(root.join("config.toml"))
}

pub async fn render_codex_provider_config(
    path: &Path,
    provider: &Provider,
) -> Result<CodexRenderedConfig, AppError> {
    let existing = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::Filesystem {
                code: "filesystem.codex_config_read",
                message: "Could not read Codex config".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            });
        }
    };

    render_codex_provider_config_from_str(path, &existing, provider)
}

pub fn render_codex_provider_config_from_str(
    path: &Path,
    existing: &str,
    provider: &Provider,
) -> Result<CodexRenderedConfig, AppError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.codex_config_path_invalid",
            message: "Codex config path must be absolute".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    let base_url = provider
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation {
            code: "validation.provider_base_url_required",
            message: "Codex real switching requires a provider base URL".to_string(),
            details: Some(provider.id.clone()),
            recoverable: true,
        })?;
    let env_key = resolve_env_key(provider)?;
    let provider_slug = codex_provider_slug(&provider.id);
    let mut document = parse_existing_toml(existing)?;

    document["model_provider"] = value(provider_slug.clone());
    if !document["model_providers"].is_table() {
        document["model_providers"] = Item::Table(Table::new());
    }

    let mut provider_table = Table::new();
    provider_table["name"] = value(provider.name.clone());
    provider_table["base_url"] = value(base_url.to_string());
    provider_table["wire_api"] = value("responses");
    provider_table["env_key"] = value(env_key);
    document["model_providers"][&provider_slug] = Item::Table(provider_table);

    Ok(CodexRenderedConfig {
        path: path.to_path_buf(),
        contents: document.to_string(),
        provider_slug,
    })
}

fn parse_existing_toml(existing: &str) -> Result<DocumentMut, AppError> {
    if existing.trim().is_empty() {
        return Ok(DocumentMut::new());
    }

    existing.parse::<DocumentMut>().map_err(|error| AppError::Validation {
        code: "validation.codex_config_toml",
        message: "Existing Codex config is not valid TOML".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn resolve_env_key(provider: &Provider) -> Result<String, AppError> {
    let value: JsonValue =
        serde_json::from_str(&provider.target_options_json).map_err(|error| {
            AppError::Validation {
                code: "validation.provider_target_options_json",
                message: "Provider target options must be a JSON object".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            }
        })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code: "validation.provider_target_options_json",
            message: "Provider target options must be a JSON object".to_string(),
            details: Some("Expected a JSON object".to_string()),
            recoverable: true,
        });
    }

    let codex_env_key = value
        .get("codex")
        .and_then(|codex| codex.get("env_key"))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty());
    let root_env_key = value
        .get("env_key")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty());

    Ok(codex_env_key
        .or(root_env_key)
        .unwrap_or("OPENAI_API_KEY")
        .to_string())
}

fn codex_provider_slug(provider_id: &str) -> String {
    let mut safe = String::new();
    let mut previous_was_underscore = false;

    for character in provider_id.chars().flat_map(char::to_lowercase) {
        let next = if character.is_ascii_alphanumeric() {
            character
        } else {
            '_'
        };

        if next == '_' {
            if !previous_was_underscore {
                safe.push(next);
            }
            previous_was_underscore = true;
        } else {
            safe.push(next);
            previous_was_underscore = false;
        }
    }

    let safe = safe.trim_matches('_');
    if safe.is_empty() {
        "ai_switch_provider".to_string()
    } else {
        format!("ai_switch_{safe}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use toml_edit::DocumentMut;

    fn provider() -> Provider {
        Provider {
            id: "Provider-1".to_string(),
            name: "Acme Provider".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{}".to_string(),
            target_options_json: "{\"codex\":{\"env_key\":\"ACME_API_KEY\"}}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_codex_home_config_path() {
        let dir = tempdir().expect("tempdir");

        let path = resolve_codex_config_path_with(Some(dir.path()), Path::new("C:/Users/example"))
            .expect("path");

        assert_eq!(path, dir.path().join("config.toml"));
    }

    #[test]
    fn renders_provider_config_and_preserves_unrelated_toml() {
        let existing = r#"
model = "gpt-5.4"

[model_providers.other]
name = "Other"
base_url = "https://other.example.com/v1"
"#;

        let rendered = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            existing,
            &provider(),
        )
        .expect("rendered");
        let parsed = rendered.contents.parse::<DocumentMut>().expect("toml");

        assert_eq!(rendered.provider_slug, "ai_switch_provider_1");
        assert_eq!(parsed["model"].as_str(), Some("gpt-5.4"));
        assert_eq!(parsed["model_provider"].as_str(), Some("ai_switch_provider_1"));
        assert_eq!(
            parsed["model_providers"]["other"]["name"].as_str(),
            Some("Other")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["base_url"].as_str(),
            Some("https://api.example.com/v1")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["env_key"].as_str(),
            Some("ACME_API_KEY")
        );
        assert_eq!(
            parsed["model_providers"]["ai_switch_provider_1"]["wire_api"].as_str(),
            Some("responses")
        );
    }

    #[test]
    fn rejects_malformed_existing_toml() {
        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "model = ",
            &provider(),
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.codex_config_toml");
    }

    #[test]
    fn rejects_missing_base_url() {
        let mut provider = provider();
        provider.base_url = None;

        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_base_url_required");
    }

    #[test]
    fn rejects_malformed_target_options_json() {
        let mut provider = provider();
        provider.target_options_json = "{".to_string();

        let error = render_codex_provider_config_from_str(
            Path::new("C:/Users/example/.codex/config.toml"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_target_options_json");
    }
}
```

- [x] **Step 4: Run adapter tests**

Run:

```powershell
pnpm rust:test codex_config
```

Expected: PASS.

- [x] **Step 5: Commit adapter**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/adapters/mod.rs src-tauri/src/adapters/codex_config.rs
git commit -m "feat: add codex provider config adapter"
```

---

### Task 2: Add Real-Mode Backend Switching

**Files:**
- Modify: `src-tauri/src/services/provider_switch_service.rs`

**Interfaces:**
- Consumes: Task 1 `render_codex_provider_config()` and `resolve_codex_config_path()`.
- Produces: `ProviderSwitchService::switch_provider()` accepting `mode = "sandbox" | "real"`.
- Produces: test-only `ProviderSwitchService::switch_provider_with_codex_config_path(pool, paths, request, codex_config_path)`.
- Preserves: `ProviderSwitchOutcome` shape.

- [x] **Step 1: Write failing backend service tests**

Append these tests to the existing `#[cfg(test)]` module in `src-tauri/src/services/provider_switch_service.rs`:

```rust
    #[tokio::test]
    async fn switch_provider_real_mode_writes_codex_config_and_records_state() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");
        let codex_config_path = codex_home.path().join("config.toml");

        let outcome = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id.clone(),
                mode: "real".to_string(),
            },
            codex_config_path.clone(),
        )
        .await
        .expect("switch");

        let written = tokio::fs::read_to_string(&codex_config_path)
            .await
            .expect("codex config");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(outcome.mode, "real");
        assert_eq!(outcome.status, "written");
        assert_eq!(outcome.path, codex_config_path.display().to_string());
        assert!(written.contains("model_provider"));
        assert!(written.contains("[model_providers.ai_switch_"));
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "written");
        assert_eq!(state.active_item_type.as_deref(), Some("provider"));
        assert_eq!(state.active_item_id.as_deref(), Some(provider.id.as_str()));
    }

    #[tokio::test]
    async fn switch_provider_real_mode_rejects_non_codex_target() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let claude = targets
            .iter()
            .find(|target| target.key == "claude_code")
            .expect("claude")
            .clone();
        let provider = seeded_provider(&pool).await;
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());

        let error = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: claude.id,
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            codex_home.path().join("config.toml"),
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.real_target_not_supported");
    }

    #[tokio::test]
    async fn switch_provider_real_mode_records_failure_after_codex_path_resolution() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let targets = TargetRepository::ensure_defaults(&pool)
            .await
            .expect("targets");
        let codex = targets
            .iter()
            .find(|target| target.key == "codex")
            .expect("codex")
            .clone();
        let provider = ProviderRepository::create(
            &pool,
            NewProvider {
                name: "Broken Provider".to_string(),
                kind: "openai_compatible".to_string(),
                base_url: None,
                model_config_json: "{}".to_string(),
                target_options_json: "{}".to_string(),
                secret_ref: None,
            },
        )
        .await
        .expect("provider");
        let data_dir = tempdir().expect("data dir");
        let codex_home = tempdir().expect("codex home");
        let paths = AppPaths::from_data_dir(data_dir.path().to_path_buf());
        paths.ensure().await.expect("paths");

        let error = ProviderSwitchService::switch_provider_with_codex_config_path(
            &pool,
            &paths,
            ProviderSwitchRequest {
                target_app_id: codex.id.clone(),
                provider_id: provider.id,
                mode: "real".to_string(),
            },
            codex_home.path().join("config.toml"),
        )
        .await
        .expect_err("error");
        let snapshot = ConfigSnapshotRepository::latest_for_target(&pool, &codex.id)
            .await
            .expect("snapshot query")
            .expect("snapshot");
        let state = TargetStateRepository::get_for_target(&pool, &codex.id)
            .await
            .expect("state");

        assert_eq!(error.code(), "validation.provider_base_url_required");
        assert_eq!(snapshot.operation, "switch_provider:real");
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.error_code.as_deref(),
            Some("validation.provider_base_url_required")
        );
        assert_eq!(state.last_write_status.as_deref(), Some("failed"));
    }
```

Run:

```powershell
pnpm rust:test provider_switch_service
```

Expected: FAIL because real-mode service helpers do not exist.

- [x] **Step 2: Implement service dispatch and real write helper**

Modify the top imports in `src-tauri/src/services/provider_switch_service.rs`:

```rust
use crate::adapters::codex_config::{render_codex_provider_config, resolve_codex_config_path};
use crate::adapters::provider_renderers::render_provider_sandbox_config;
```

Replace `ProviderSwitchService::switch_provider` with:

```rust
    pub async fn switch_provider(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        let codex_config_path = if request.mode == "real" {
            Some(resolve_codex_config_path()?)
        } else {
            None
        };

        Self::switch_provider_inner(pool, paths, request, codex_config_path).await
    }

    #[cfg(test)]
    pub async fn switch_provider_with_codex_config_path(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
        codex_config_path: PathBuf,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        Self::switch_provider_inner(pool, paths, request, Some(codex_config_path)).await
    }

    async fn switch_provider_inner(
        pool: &SqlitePool,
        paths: &AppPaths,
        request: ProviderSwitchRequest,
        codex_config_path: Option<PathBuf>,
    ) -> Result<ProviderSwitchOutcome, AppError> {
        let target = TargetRepository::get(pool, &request.target_app_id).await?;
        let provider = ProviderRepository::get(pool, &request.provider_id).await?;

        if target.enabled == 0 {
            return Err(AppError::Validation {
                code: "validation.target_disabled",
                message: "Target app is disabled".to_string(),
                details: Some(target.key),
                recoverable: true,
            });
        }

        match request.mode.as_str() {
            "sandbox" => switch_provider_sandbox(pool, paths, target, provider).await,
            "real" => {
                let path = codex_config_path.ok_or_else(|| AppError::Filesystem {
                    code: "filesystem.codex_config_path_invalid",
                    message: "Could not resolve Codex config path".to_string(),
                    details: None,
                    recoverable: false,
                })?;
                switch_provider_real_codex(pool, target, provider, path).await
            }
            _ => Err(AppError::Validation {
                code: "validation.switch_mode",
                message: "Provider switching supports sandbox or real mode".to_string(),
                details: Some(request.mode),
                recoverable: true,
            }),
        }
    }
```

Move the existing sandbox body into a new helper below the `impl ProviderSwitchService` block:

```rust
async fn switch_provider_sandbox(
    pool: &SqlitePool,
    paths: &AppPaths,
    target: TargetApp,
    provider: Provider,
) -> Result<ProviderSwitchOutcome, AppError> {
    let path = sandbox_provider_path(paths, &target)?;
    let written_at = Utc::now().to_rfc3339();
    let rendered = match render_provider_sandbox_config(&target, &provider) {
        Ok(rendered) => rendered,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:sandbox",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };
    let write_outcome = match ConfigWriter::write_atomic(&path, &rendered).await {
        Ok(outcome) => outcome,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:sandbox",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };

    record_successful_attempt(pool, target, provider, "sandbox", "switch_provider:sandbox", write_outcome, written_at).await
}
```

Add the real Codex helper:

```rust
async fn switch_provider_real_codex(
    pool: &SqlitePool,
    target: TargetApp,
    provider: Provider,
    path: PathBuf,
) -> Result<ProviderSwitchOutcome, AppError> {
    if target.key != "codex" {
        return Err(AppError::Validation {
            code: "validation.real_target_not_supported",
            message: "Real provider switching is only available for Codex in B2.1".to_string(),
            details: Some(target.key),
            recoverable: true,
        });
    }

    let written_at = Utc::now().to_rfc3339();
    let rendered = match render_codex_provider_config(&path, &provider).await {
        Ok(rendered) => rendered,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };
    let write_outcome = match ConfigWriter::write_atomic(&rendered.path, &rendered.contents).await {
        Ok(outcome) => outcome,
        Err(error) => {
            record_failed_attempt(
                pool,
                &target,
                &path,
                "switch_provider:real",
                error.code(),
                &written_at,
            )
            .await;
            return Err(error);
        }
    };

    record_successful_attempt(pool, target, provider, "real", "switch_provider:real", write_outcome, written_at).await
}
```

Add the shared success helper and update failure helper:

```rust
async fn record_successful_attempt(
    pool: &SqlitePool,
    target: TargetApp,
    provider: Provider,
    mode: &str,
    operation: &str,
    write_outcome: crate::config_writer::WriteOutcome,
    written_at: String,
) -> Result<ProviderSwitchOutcome, AppError> {
    let snapshot = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: operation.to_string(),
            path: write_outcome.path.clone(),
            before_hash: write_outcome.before_hash.clone(),
            after_hash: write_outcome.after_hash.clone(),
            backup_path: None,
            status: "written".to_string(),
            error_code: None,
        },
    )
    .await?;
    let state = TargetStateRepository::upsert_provider_state(
        pool,
        &target.id,
        &provider.id,
        "written",
        None,
        &written_at,
    )
    .await?;

    Ok(ProviderSwitchOutcome {
        target_app_id: target.id,
        target_key: target.key,
        provider_id: provider.id,
        provider_name: provider.name,
        mode: mode.to_string(),
        path: write_outcome.path,
        status: "written".to_string(),
        before_hash: write_outcome.before_hash,
        after_hash: write_outcome.after_hash,
        snapshot_id: snapshot.id,
        state_id: state.id,
        written_at,
    })
}
```

Change `record_failed_attempt` signature and operation field:

```rust
async fn record_failed_attempt(
    pool: &SqlitePool,
    target: &TargetApp,
    path: &PathBuf,
    operation: &str,
    error_code: &str,
    written_at: &str,
) {
    let _ = ConfigSnapshotRepository::insert(
        pool,
        NewConfigSnapshot {
            target_app_id: Some(target.id.clone()),
            operation: operation.to_string(),
            path: path.display().to_string(),
            before_hash: None,
            after_hash: None,
            backup_path: None,
            status: "failed".to_string(),
            error_code: Some(error_code.to_string()),
        },
    )
    .await;
    let _ = TargetStateRepository::record_failure(pool, &target.id, error_code, written_at).await;
}
```

- [x] **Step 3: Run backend service tests**

Run:

```powershell
pnpm rust:test provider_switch_service
pnpm rust:test codex_config
```

Expected: PASS.

- [x] **Step 4: Commit backend real mode**

```powershell
git add src-tauri/src/services/provider_switch_service.rs
git commit -m "feat: add codex real provider switching"
```

---

### Task 3: Add Frontend Real Codex Switching UI

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `tests/apiClient.test.ts`
- Modify: `tests/ProvidersScreen.test.tsx`
- Modify: `src/screens/ProvidersScreen.tsx`

**Interfaces:**
- Consumes: existing `switchTargetProvider(request)`.
- Produces: TypeScript `ProviderSwitchRequest.mode: "sandbox" | "real"`.
- Produces: `Switch Codex config` button only for selected Codex target.

- [x] **Step 1: Update API types**

Change `src/lib/api/types.ts`:

```ts
export type ProviderSwitchRequest = {
  target_app_id: string;
  provider_id: string;
  mode: "sandbox" | "real";
};
```

Change `ProviderSwitchOutcome.mode`:

```ts
  mode: "sandbox" | "real";
```

- [x] **Step 2: Update API client test for real mode**

Append this assertion inside `tests/apiClient.test.ts` after the existing sandbox switch assertion:

```ts
    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-codex",
      provider_id: "provider-1",
      mode: "real",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "real",
      },
    });
```

Run:

```powershell
pnpm test:run tests/apiClient.test.ts
```

Expected: PASS after Step 1.

- [x] **Step 3: Write failing ProvidersScreen real-mode tests**

Append these tests to `tests/ProvidersScreen.test.tsx`:

```tsx
  it("shows a real Codex switch action when Codex is selected", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);
    vi.mocked(switchTargetProvider).mockResolvedValueOnce({
      target_app_id: "target-codex",
      target_key: "codex",
      provider_id: "provider-1",
      provider_name: "Acme Provider",
      mode: "real",
      path: "C:/Users/example/.codex/config.toml",
      status: "written",
      before_hash: null,
      after_hash: "after",
      snapshot_id: "snapshot-real",
      state_id: "state-1",
      written_at: "2026-07-13T00:00:00Z",
    });

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-codex");
    await userEvent.click(screen.getByRole("button", { name: "Switch Acme Provider Codex config" }));

    await waitFor(() => {
      expect(switchTargetProvider).toHaveBeenCalledWith({
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "real",
      });
    });
    expect(await screen.findByText("Wrote Codex config for Acme Provider to Codex.")).toBeInTheDocument();
  });

  it("hides the real Codex switch action for non-Codex targets", async () => {
    vi.mocked(listProviders).mockResolvedValueOnce(providersFixture);
    vi.mocked(listTargetSwitchStatuses).mockResolvedValue(targetSwitchStatusesFixture);

    renderWithClient();

    expect(await screen.findByText("Acme Provider")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Target for Acme Provider"), "target-claude");

    expect(
      screen.queryByRole("button", { name: "Switch Acme Provider Codex config" }),
    ).not.toBeInTheDocument();
  });
```

Run:

```powershell
pnpm test:run tests/ProvidersScreen.test.tsx
```

Expected: FAIL because the UI does not render real Codex controls yet.

- [x] **Step 4: Implement ProvidersScreen real action**

In `src/screens/ProvidersScreen.tsx`, change the intro copy:

```tsx
        <p className="text-steel">
          Switch a provider into sandbox configs, or write Codex user config explicitly.
        </p>
```

Inside the provider map, after `selectedStatus` is declared, add:

```tsx
          const canSwitchRealCodex = selectedStatus?.target.key === "codex";
```

After the existing sandbox `<Button>`, add:

```tsx
                  {canSwitchRealCodex && (
                    <Button
                      type="button"
                      disabled={!selectedTargetId || switchMutation.isPending}
                      aria-label={`Switch ${provider.name} Codex config`}
                      className="cursor-pointer bg-ink text-white hover:bg-ink/90 disabled:cursor-not-allowed disabled:opacity-60"
                      onClick={() =>
                        switchMutation.mutate({
                          target_app_id: selectedTargetId,
                          provider_id: provider.id,
                          mode: "real",
                        })
                      }
                    >
                      Switch Codex config
                    </Button>
                  )}
```

Replace the success copy with:

```tsx
          {switchMutation.data.mode === "real" ? "Wrote Codex config" : "Wrote sandbox config"} for{" "}
          {switchMutation.data.provider_name} to {switchedTargetName}.
```

Replace the error copy with:

```tsx
          Provider switch failed.
```

- [x] **Step 5: Run frontend tests and typecheck**

Run:

```powershell
pnpm test:run tests/apiClient.test.ts tests/ProvidersScreen.test.tsx
pnpm typecheck
```

Expected: PASS.

- [x] **Step 6: Commit frontend real-mode UI**

```powershell
git add src/lib/api/types.ts tests/apiClient.test.ts tests/ProvidersScreen.test.tsx src/screens/ProvidersScreen.tsx
git commit -m "feat: add codex real switching ui"
```

---

### Task 4: Document Safe Smoke Flow And Full Verification

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: B2.1 backend and frontend behavior.
- Produces: README smoke instructions using temporary `CODEX_HOME`.

- [x] **Step 1: Update README B2.1 notes**

Append this section to `README.md`:

````markdown
## Provider Switching B2.1: Codex Real Mode

B2.1 adds explicit real provider switching for Codex only. Sandbox switching remains available for all supported targets.

Codex real mode writes:

```text
<CODEX_HOME>/config.toml
```

If `CODEX_HOME` is not set, the app uses:

```text
~/.codex/config.toml
```

The Codex config contains provider metadata such as `model_provider`, `base_url`, `wire_api`, and `env_key`. It does not store raw API keys.

Safe smoke test:

1. Set `CODEX_HOME` to a temporary directory.
2. Start the app with `pnpm tauri:dev`.
3. Import or create a provider with `base_url`.
4. Open `Providers`.
5. Select `Codex`.
6. Click `Switch Codex config`.
7. Verify `<CODEX_HOME>/config.toml` contains `model_provider` and `[model_providers.ai_switch_<id>]`.
8. Verify your real `~/.codex/config.toml` was not modified when using temporary `CODEX_HOME`.
````

- [x] **Step 2: Run full verification**

Run:

```powershell
git status --short
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

Expected:

- `git status --short` shows only the README change before the final commit.
- Frontend tests pass.
- TypeScript typecheck passes.
- Rust check passes.
- Rust tests pass.

- [x] **Step 3: Commit docs**

```powershell
git add README.md
git commit -m "docs: add codex real switching smoke notes"
```

- [x] **Step 4: Optional manual smoke**

Run only when an interactive app window is acceptable:

```powershell
$env:CODEX_HOME = Join-Path $env:TEMP "ai-switch-codex-smoke"
pnpm tauri:dev
```

Expected:

- App starts.
- A provider with `base_url` can be imported or created.
- Providers screen can switch that provider to Codex real mode.
- `<CODEX_HOME>/config.toml` exists.
- The file contains `model_provider` and `[model_providers.ai_switch_<id>]`.
- No real config outside temporary `CODEX_HOME` is modified.

---

## Final Implementation Verification

After all tasks complete, run:

```powershell
git status --short
pnpm test:run
pnpm typecheck
pnpm rust:check
pnpm rust:test
```

Expected:

- `git status --short` is clean after final commit.
- Frontend tests pass.
- TypeScript typecheck passes.
- Rust check passes.
- Rust tests pass.

Manual smoke remains optional because `pnpm tauri:dev` is interactive/blocking. If skipped, report that it was skipped and give the temporary `CODEX_HOME` command above.

## Spec Coverage Map

- Add real mode for Codex only: Task 2.
- Keep sandbox unchanged: Task 2 regression tests and full verification.
- Backend path resolution: Task 1.
- Codex TOML rendering: Task 1.
- No raw API keys: Task 1 renders `env_key` only.
- Use `ConfigWriter`: Task 2.
- Record real snapshots and state: Task 2.
- Frontend distinguishes sandbox and real actions: Task 3.
- Safe smoke documentation: Task 4.
