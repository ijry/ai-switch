use crate::error::AppError;
use crate::models::provider::Provider;
use directories::BaseDirs;
use serde_json::{Map, Value as JsonValue};
use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_ENV_KEY: &str = "ANTHROPIC_API_KEY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeRenderedConfig {
    pub path: PathBuf,
    pub contents: String,
    pub model_id: String,
    pub env_key: String,
}

pub fn resolve_claude_code_config_path() -> Result<PathBuf, AppError> {
    let home_dir = BaseDirs::new()
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?
        .home_dir()
        .to_path_buf();
    let claude_config_dir = env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    resolve_claude_code_config_path_with(claude_config_dir.as_deref(), &home_dir)
}

pub fn resolve_claude_code_config_path_with(
    claude_config_dir: Option<&Path>,
    home_dir: &Path,
) -> Result<PathBuf, AppError> {
    let root = claude_config_dir
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home_dir.join(".claude"));

    if root.as_os_str().is_empty() || !root.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.claude_code_config_path_invalid",
            message: "Claude Code config directory must be an absolute path".to_string(),
            details: Some(root.display().to_string()),
            recoverable: false,
        });
    }

    Ok(root.join("settings.json"))
}

pub async fn render_claude_code_provider_config(
    path: &Path,
    provider: &Provider,
) -> Result<ClaudeCodeRenderedConfig, AppError> {
    let existing = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::Filesystem {
                code: "filesystem.claude_code_config_read",
                message: "Could not read Claude Code settings".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            });
        }
    };

    render_claude_code_provider_config_from_str(path, &existing, provider)
}

pub fn render_claude_code_provider_config_from_str(
    path: &Path,
    existing: &str,
    provider: &Provider,
) -> Result<ClaudeCodeRenderedConfig, AppError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.claude_code_config_path_invalid",
            message: "Claude Code settings path must be absolute".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    let model_config = parse_json_object(
        &provider.model_config_json,
        "validation.provider_model_config_json",
        "Provider model configuration must be a JSON object",
    )?;
    let target_options = parse_json_object(
        &provider.target_options_json,
        "validation.provider_target_options_json",
        "Provider target options must be a JSON object",
    )?;
    let base_url = resolve_base_url(provider, &target_options)?;
    let model_id = resolve_model_id(&model_config, &target_options)?;
    let small_fast_model = resolve_small_fast_model(&model_config, &target_options);
    let env_key = resolve_env_key(&target_options);
    let api_key_helper = resolve_api_key_helper(&target_options, &env_key)?;
    let mut root = parse_existing_config(existing)?;

    let env_value = root
        .entry("env".to_string())
        .or_insert_with(|| JsonValue::Object(Map::new()));
    let env = env_value
        .as_object_mut()
        .ok_or_else(|| AppError::Validation {
            code: "validation.claude_code_config_json",
            message: "Claude Code settings env field must be a JSON object".to_string(),
            details: None,
            recoverable: true,
        })?;
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        JsonValue::String(base_url.clone()),
    );
    env.insert(
        "ANTHROPIC_MODEL".to_string(),
        JsonValue::String(model_id.clone()),
    );
    if let Some(small_fast_model) = small_fast_model {
        env.insert(
            "ANTHROPIC_SMALL_FAST_MODEL".to_string(),
            JsonValue::String(small_fast_model),
        );
    }

    root.insert(
        "apiKeyHelper".to_string(),
        JsonValue::String(api_key_helper),
    );
    root.insert(
        "aiSwitch".to_string(),
        JsonValue::Object(render_ai_switch_metadata(
            provider, &base_url, &model_id, &env_key,
        )),
    );

    let mut contents = serde_json::to_string_pretty(&JsonValue::Object(root)).map_err(|error| {
        AppError::Validation {
            code: "validation.claude_code_config_json",
            message: "Could not render Claude Code settings JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        }
    })?;
    contents.push('\n');

    Ok(ClaudeCodeRenderedConfig {
        path: path.to_path_buf(),
        contents,
        model_id,
        env_key,
    })
}

fn render_ai_switch_metadata(
    provider: &Provider,
    base_url: &str,
    model_id: &str,
    env_key: &str,
) -> Map<String, JsonValue> {
    let mut active_provider = Map::new();
    active_provider.insert("id".to_string(), JsonValue::String(provider.id.clone()));
    active_provider.insert("name".to_string(), JsonValue::String(provider.name.clone()));
    active_provider.insert("kind".to_string(), JsonValue::String(provider.kind.clone()));
    active_provider.insert("baseUrl".to_string(), JsonValue::String(base_url.to_string()));
    active_provider.insert("model".to_string(), JsonValue::String(model_id.to_string()));
    active_provider.insert("envKey".to_string(), JsonValue::String(env_key.to_string()));

    let mut metadata = Map::new();
    metadata.insert(
        "activeProvider".to_string(),
        JsonValue::Object(active_provider),
    );
    metadata
}

fn parse_existing_config(existing: &str) -> Result<Map<String, JsonValue>, AppError> {
    if existing.trim().is_empty() {
        return Ok(Map::new());
    }

    let value: JsonValue =
        serde_json::from_str(existing).map_err(|error| AppError::Validation {
            code: "validation.claude_code_config_json",
            message: "Existing Claude Code settings are not valid JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    value
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::Validation {
            code: "validation.claude_code_config_json",
            message: "Existing Claude Code settings must be a JSON object".to_string(),
            details: Some("Expected a JSON object".to_string()),
            recoverable: true,
        })
}

fn parse_json_object(raw: &str, code: &'static str, message: &str) -> Result<JsonValue, AppError> {
    let value: JsonValue = serde_json::from_str(raw).map_err(|error| AppError::Validation {
        code,
        message: message.to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code,
            message: message.to_string(),
            details: Some("Expected a JSON object".to_string()),
            recoverable: true,
        });
    }

    Ok(value)
}

fn resolve_base_url(provider: &Provider, target_options: &JsonValue) -> Result<String, AppError> {
    nested_string(target_options, "claude_code", "base_url")
        .or_else(|| string_at(target_options, "base_url"))
        .or_else(|| provider.base_url.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation {
            code: "validation.provider_base_url_required",
            message: "Claude Code real switching requires a provider base URL".to_string(),
            details: Some(provider.id.clone()),
            recoverable: true,
        })
}

fn resolve_model_id(
    model_config: &JsonValue,
    target_options: &JsonValue,
) -> Result<String, AppError> {
    nested_string(target_options, "claude_code", "model")
        .or_else(|| string_at(target_options, "model"))
        .or_else(|| nested_string(model_config, "claude_code", "model"))
        .or_else(|| nested_string(model_config, "claude_code", "default"))
        .or_else(|| string_at(model_config, "default"))
        .or_else(|| string_at(model_config, "model"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation {
            code: "validation.provider_model_required",
            message: "Claude Code real switching requires a model id".to_string(),
            details: Some(
                "Set model_config_json.default or target_options_json.claude_code.model"
                    .to_string(),
            ),
            recoverable: true,
        })
}

fn resolve_small_fast_model(
    model_config: &JsonValue,
    target_options: &JsonValue,
) -> Option<String> {
    nested_string(target_options, "claude_code", "small_fast_model")
        .or_else(|| string_at(target_options, "small_fast_model"))
        .or_else(|| nested_string(model_config, "claude_code", "small_fast_model"))
        .or_else(|| string_at(model_config, "small_fast_model"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_env_key(target_options: &JsonValue) -> String {
    nested_string(target_options, "claude_code", "env_key")
        .or_else(|| string_at(target_options, "env_key"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ENV_KEY.to_string())
}

fn resolve_api_key_helper(target_options: &JsonValue, env_key: &str) -> Result<String, AppError> {
    if let Some(helper) = nested_string(target_options, "claude_code", "api_key_helper")
        .or_else(|| string_at(target_options, "api_key_helper"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(helper);
    }

    validate_env_key(env_key)?;
    Ok(format!(
        "node -e \"process.stdout.write(process.env.{env_key} || '')\""
    ))
}

fn validate_env_key(env_key: &str) -> Result<(), AppError> {
    let mut characters = env_key.chars();
    let first = characters.next().ok_or_else(|| AppError::Validation {
        code: "validation.provider_env_key_invalid",
        message: "Claude Code API key helper requires an environment variable name".to_string(),
        details: Some(env_key.to_string()),
        recoverable: true,
    })?;

    if !(first == '_' || first.is_ascii_alphabetic())
        || !characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(AppError::Validation {
            code: "validation.provider_env_key_invalid",
            message: "Claude Code API key helper requires an environment variable name".to_string(),
            details: Some(env_key.to_string()),
            recoverable: true,
        });
    }

    Ok(())
}

fn nested_string(value: &JsonValue, section: &str, key: &str) -> Option<String> {
    value
        .get(section)
        .and_then(|section| section.get(key))
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
}

fn string_at(value: &JsonValue, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::tempdir;

    fn provider() -> Provider {
        Provider {
            id: "Provider-1".to_string(),
            name: "Acme Provider".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{\"default\":\"claude-3-5-sonnet-latest\"}".to_string(),
            target_options_json: "{}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_claude_config_dir_settings_path() {
        let dir = tempdir().expect("tempdir");

        let path =
            resolve_claude_code_config_path_with(Some(dir.path()), Path::new("C:/Users/example"))
                .expect("path");

        assert_eq!(path, dir.path().join("settings.json"));
    }

    #[test]
    fn renders_provider_config_and_preserves_unrelated_settings() {
        let existing = r#"{
  "permissions": {
    "allow": ["Bash(ls:*)"]
  },
  "env": {
    "KEEP_ME": "yes",
    "ANTHROPIC_MODEL": "old-model"
  }
}"#;

        let rendered = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            existing,
            &provider(),
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(rendered.model_id, "claude-3-5-sonnet-latest");
        assert_eq!(rendered.env_key, "ANTHROPIC_API_KEY");
        assert_eq!(parsed["permissions"]["allow"][0], "Bash(ls:*)");
        assert_eq!(parsed["env"]["KEEP_ME"], "yes");
        assert_eq!(parsed["env"]["ANTHROPIC_BASE_URL"], "https://api.example.com/v1");
        assert_eq!(parsed["env"]["ANTHROPIC_MODEL"], "claude-3-5-sonnet-latest");
        assert!(
            parsed["apiKeyHelper"]
                .as_str()
                .expect("api key helper")
                .contains("ANTHROPIC_API_KEY")
        );
        assert_eq!(parsed["aiSwitch"]["activeProvider"]["id"], "Provider-1");
        assert_eq!(
            parsed["aiSwitch"]["activeProvider"]["envKey"],
            "ANTHROPIC_API_KEY"
        );
        assert!(rendered.contents.ends_with('\n'));
        assert!(!rendered.contents.contains("secret://provider/acme"));
    }

    #[test]
    fn target_options_override_model_base_url_helper_and_small_fast_model() {
        let mut provider = provider();
        provider.target_options_json = r#"{
  "claude_code": {
    "model": "claude-sonnet-4-20250514",
    "small_fast_model": "claude-3-5-haiku-latest",
    "base_url": "https://claude-proxy.example.com",
    "env_key": "ACME_CLAUDE_KEY",
    "api_key_helper": "op read op://vault/acme/api-key"
  }
}"#
        .to_string();

        let rendered = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            "{}",
            &provider,
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(parsed["env"]["ANTHROPIC_BASE_URL"], "https://claude-proxy.example.com");
        assert_eq!(parsed["env"]["ANTHROPIC_MODEL"], "claude-sonnet-4-20250514");
        assert_eq!(
            parsed["env"]["ANTHROPIC_SMALL_FAST_MODEL"],
            "claude-3-5-haiku-latest"
        );
        assert_eq!(
            parsed["apiKeyHelper"],
            "op read op://vault/acme/api-key"
        );
        assert_eq!(parsed["aiSwitch"]["activeProvider"]["envKey"], "ACME_CLAUDE_KEY");
    }

    #[test]
    fn rejects_malformed_existing_json() {
        let error = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            "{",
            &provider(),
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.claude_code_config_json");
    }

    #[test]
    fn rejects_missing_base_url() {
        let mut provider = provider();
        provider.base_url = None;

        let error = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            "{}",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_base_url_required");
    }

    #[test]
    fn rejects_missing_model_id() {
        let mut provider = provider();
        provider.model_config_json = "{}".to_string();

        let error = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            "{}",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_model_required");
    }

    #[test]
    fn rejects_invalid_default_helper_env_key() {
        let mut provider = provider();
        provider.target_options_json = "{\"claude_code\":{\"env_key\":\"bad-name\"}}".to_string();

        let error = render_claude_code_provider_config_from_str(
            Path::new("C:/Users/example/.claude/settings.json"),
            "{}",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_env_key_invalid");
    }
}
