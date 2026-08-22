use crate::error::AppError;
use crate::models::provider::Provider;
use directories::BaseDirs;
use serde_json::{Map, Value as JsonValue};
use std::env;
use std::path::{Path, PathBuf};

const DEFAULT_ENV_KEY: &str = "GEMINI_API_KEY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiRenderedConfig {
    pub path: PathBuf,
    pub contents: String,
    pub model_id: String,
}

pub fn resolve_gemini_config_path() -> Result<PathBuf, AppError> {
    let home_dir = BaseDirs::new()
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?
        .home_dir()
        .to_path_buf();
    let custom_config = env::var_os("GEMINI_CLI_SETTINGS")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    resolve_gemini_config_path_with(custom_config.as_deref(), &home_dir)
}

pub fn resolve_gemini_config_path_with(
    custom_config: Option<&Path>,
    home_dir: &Path,
) -> Result<PathBuf, AppError> {
    let path = custom_config
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home_dir.join(".gemini").join("settings.json"));

    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.gemini_config_path_invalid",
            message: "Gemini CLI settings path must be absolute".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    Ok(path)
}

pub async fn render_gemini_provider_config(
    path: &Path,
    provider: &Provider,
) -> Result<GeminiRenderedConfig, AppError> {
    let existing = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::Filesystem {
                code: "filesystem.gemini_config_read",
                message: "Could not read Gemini CLI settings".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            });
        }
    };

    render_gemini_provider_config_from_str(path, &existing, provider)
}

pub fn render_gemini_provider_config_from_str(
    path: &Path,
    existing: &str,
    provider: &Provider,
) -> Result<GeminiRenderedConfig, AppError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.gemini_config_path_invalid",
            message: "Gemini CLI settings path must be absolute".to_string(),
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
    let model_id = resolve_model_id(&model_config, &target_options)?;
    let env_key = resolve_env_key(&target_options);
    let mut root = parse_existing_config(existing)?;

    let model_value = root
        .entry("model".to_string())
        .or_insert_with(|| JsonValue::Object(Map::new()));
    let model = model_value
        .as_object_mut()
        .ok_or_else(|| AppError::Validation {
            code: "validation.gemini_config_json",
            message: "Gemini CLI settings model field must be a JSON object".to_string(),
            details: None,
            recoverable: true,
        })?;
    model.insert("name".to_string(), JsonValue::String(model_id.clone()));

    root.insert(
        "aiSwitch".to_string(),
        JsonValue::Object(render_ai_switch_metadata(provider, &env_key)),
    );

    let mut contents = serde_json::to_string_pretty(&JsonValue::Object(root)).map_err(|error| {
        AppError::Validation {
            code: "validation.gemini_config_json",
            message: "Could not render Gemini CLI settings JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        }
    })?;
    contents.push('\n');

    Ok(GeminiRenderedConfig {
        path: path.to_path_buf(),
        contents,
        model_id,
    })
}

fn render_ai_switch_metadata(provider: &Provider, env_key: &str) -> Map<String, JsonValue> {
    let mut active_provider = Map::new();
    active_provider.insert("id".to_string(), JsonValue::String(provider.id.clone()));
    active_provider.insert("name".to_string(), JsonValue::String(provider.name.clone()));
    active_provider.insert("kind".to_string(), JsonValue::String(provider.kind.clone()));
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
            code: "validation.gemini_config_json",
            message: "Existing Gemini CLI settings are not valid JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    value
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::Validation {
            code: "validation.gemini_config_json",
            message: "Existing Gemini CLI settings must be a JSON object".to_string(),
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

fn resolve_model_id(
    model_config: &JsonValue,
    target_options: &JsonValue,
) -> Result<String, AppError> {
    nested_string(target_options, "gemini_cli", "model")
        .or_else(|| string_at(target_options, "model"))
        .or_else(|| nested_string(model_config, "gemini_cli", "model"))
        .or_else(|| string_at(model_config, "default"))
        .or_else(|| string_at(model_config, "model"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation {
            code: "validation.provider_model_required",
            message: "Gemini CLI real switching requires a model id".to_string(),
            details: Some(
                "Set model_config_json.default or target_options_json.gemini_cli.model".to_string(),
            ),
            recoverable: true,
        })
}

fn resolve_env_key(target_options: &JsonValue) -> String {
    nested_string(target_options, "gemini_cli", "env_key")
        .or_else(|| string_at(target_options, "env_key"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ENV_KEY.to_string())
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
            model_config_json: "{\"default\":\"gemini-2.5-flash\"}".to_string(),
            target_options_json: "{}".to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_custom_gemini_config_path() {
        let dir = tempdir().expect("tempdir");
        let custom_path = dir.path().join("settings.json");

        let path =
            resolve_gemini_config_path_with(Some(&custom_path), Path::new("C:/Users/example"))
                .expect("path");

        assert_eq!(path, custom_path);
    }

    #[test]
    fn renders_model_and_preserves_unrelated_json() {
        let existing = r#"{"ui":{"theme":"dark"},"model":{"maxSessionTurns":5}}"#;

        let rendered = render_gemini_provider_config_from_str(
            Path::new("C:/Users/example/.gemini/settings.json"),
            existing,
            &provider(),
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(rendered.model_id, "gemini-2.5-flash");
        assert_eq!(parsed["ui"]["theme"], "dark");
        assert_eq!(parsed["model"]["maxSessionTurns"], 5);
        assert_eq!(parsed["model"]["name"], "gemini-2.5-flash");
        assert_eq!(parsed["aiSwitch"]["activeProvider"]["id"], "Provider-1");
        assert_eq!(
            parsed["aiSwitch"]["activeProvider"]["envKey"],
            DEFAULT_ENV_KEY
        );
        assert!(!rendered.contents.contains("secret://provider/acme"));
    }

    #[test]
    fn uses_target_specific_model_and_env_key() {
        let mut provider = provider();
        provider.target_options_json =
            "{\"gemini_cli\":{\"model\":\"gemini-2.5-pro\",\"env_key\":\"GOOGLE_API_KEY\"}}"
                .to_string();

        let rendered = render_gemini_provider_config_from_str(
            Path::new("C:/Users/example/.gemini/settings.json"),
            "",
            &provider,
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(rendered.model_id, "gemini-2.5-pro");
        assert_eq!(parsed["model"]["name"], "gemini-2.5-pro");
        assert_eq!(
            parsed["aiSwitch"]["activeProvider"]["envKey"],
            "GOOGLE_API_KEY"
        );
    }

    #[test]
    fn rejects_malformed_existing_json() {
        let error = render_gemini_provider_config_from_str(
            Path::new("C:/Users/example/.gemini/settings.json"),
            "{\"model\":",
            &provider(),
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.gemini_config_json");
    }

    #[test]
    fn rejects_malformed_target_options_json() {
        let mut provider = provider();
        provider.target_options_json = "{".to_string();

        let error = render_gemini_provider_config_from_str(
            Path::new("C:/Users/example/.gemini/settings.json"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_target_options_json");
    }

    #[test]
    fn rejects_missing_model_id() {
        let mut provider = provider();
        provider.model_config_json = "{}".to_string();
        provider.target_options_json = "{}".to_string();

        let error = render_gemini_provider_config_from_str(
            Path::new("C:/Users/example/.gemini/settings.json"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_model_required");
    }
}
