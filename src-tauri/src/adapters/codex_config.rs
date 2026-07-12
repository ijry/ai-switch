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
    let has_model_providers_table = document
        .as_table()
        .get("model_providers")
        .map(Item::is_table)
        .unwrap_or(false);
    if !has_model_providers_table {
        document
            .as_table_mut()
            .insert("model_providers", Item::Table(Table::new()));
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

    existing
        .parse::<DocumentMut>()
        .map_err(|error| AppError::Validation {
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
        assert_eq!(
            parsed["model_provider"].as_str(),
            Some("ai_switch_provider_1")
        );
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
