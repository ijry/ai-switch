use crate::error::AppError;
use crate::models::provider::Provider;
use directories::BaseDirs;
use serde_json::{Map, Value as JsonValue};
use std::env;
use std::path::{Path, PathBuf};

const OPENCODE_SCHEMA: &str = "https://opencode.ai/config.json";
const DEFAULT_NPM_PACKAGE: &str = "@ai-sdk/openai-compatible";
const DEFAULT_ENV_KEY: &str = "OPENAI_API_KEY";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCodeRenderedConfig {
    pub path: PathBuf,
    pub contents: String,
    pub provider_slug: String,
    pub model_id: String,
}

pub fn resolve_opencode_config_path() -> Result<PathBuf, AppError> {
    let home_dir = BaseDirs::new()
        .ok_or_else(|| AppError::Filesystem {
            code: "filesystem.home_not_found",
            message: "Could not resolve the current user home directory".to_string(),
            details: None,
            recoverable: false,
        })?
        .home_dir()
        .to_path_buf();
    let custom_config = env::var_os("OPENCODE_CONFIG")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    resolve_opencode_config_path_with(custom_config.as_deref(), &home_dir)
}

pub fn resolve_opencode_config_path_with(
    custom_config: Option<&Path>,
    home_dir: &Path,
) -> Result<PathBuf, AppError> {
    let path = custom_config
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            home_dir
                .join(".config")
                .join("opencode")
                .join("opencode.json")
        });

    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.opencode_config_path_invalid",
            message: "OpenCode config path must be absolute".to_string(),
            details: Some(path.display().to_string()),
            recoverable: false,
        });
    }

    Ok(path)
}

pub async fn render_opencode_provider_config(
    path: &Path,
    provider: &Provider,
) -> Result<OpenCodeRenderedConfig, AppError> {
    let existing = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::Filesystem {
                code: "filesystem.opencode_config_read",
                message: "Could not read OpenCode config".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            });
        }
    };

    render_opencode_provider_config_from_str(path, &existing, provider)
}

pub fn render_opencode_provider_config_from_str(
    path: &Path,
    existing: &str,
    provider: &Provider,
) -> Result<OpenCodeRenderedConfig, AppError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::Filesystem {
            code: "filesystem.opencode_config_path_invalid",
            message: "OpenCode config path must be absolute".to_string(),
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
            message: "OpenCode real switching requires a provider base URL".to_string(),
            details: Some(provider.id.clone()),
            recoverable: true,
        })?;
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
    let env_key = resolve_env_key(&target_options);
    let npm_package = resolve_npm_package(&target_options);
    let provider_slug = opencode_provider_slug(&provider.id);
    let provider_name = nested_string(&target_options, "opencode", "provider_name")
        .unwrap_or_else(|| provider.name.clone());
    let model_id = resolve_model_id(&model_config, &target_options)?;
    let model_name = nested_string(&target_options, "opencode", "model_name")
        .or_else(|| string_at(&model_config, "model_name"))
        .unwrap_or_else(|| model_id.clone());
    let mut root = parse_existing_config(existing)?;

    root.entry("$schema".to_string())
        .or_insert_with(|| JsonValue::String(OPENCODE_SCHEMA.to_string()));
    root.insert(
        "model".to_string(),
        JsonValue::String(format!("{provider_slug}/{model_id}")),
    );

    let provider_value = root
        .entry("provider".to_string())
        .or_insert_with(|| JsonValue::Object(Map::new()));
    let providers = provider_value
        .as_object_mut()
        .ok_or_else(|| AppError::Validation {
            code: "validation.opencode_config_json",
            message: "OpenCode config provider field must be a JSON object".to_string(),
            details: None,
            recoverable: true,
        })?;

    providers.insert(
        provider_slug.clone(),
        JsonValue::Object(render_provider_block(
            &provider_name,
            &npm_package,
            base_url,
            &env_key,
            &model_id,
            &model_name,
        )),
    );

    let mut contents = serde_json::to_string_pretty(&JsonValue::Object(root)).map_err(|error| {
        AppError::Validation {
            code: "validation.opencode_config_json",
            message: "Could not render OpenCode config JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        }
    })?;
    contents.push('\n');

    Ok(OpenCodeRenderedConfig {
        path: path.to_path_buf(),
        contents,
        provider_slug,
        model_id,
    })
}

fn render_provider_block(
    provider_name: &str,
    npm_package: &str,
    base_url: &str,
    env_key: &str,
    model_id: &str,
    model_name: &str,
) -> Map<String, JsonValue> {
    let mut options = Map::new();
    options.insert(
        "baseURL".to_string(),
        JsonValue::String(base_url.to_string()),
    );
    options.insert(
        "apiKey".to_string(),
        JsonValue::String(format!("{{env:{env_key}}}")),
    );

    let mut model_details = Map::new();
    model_details.insert(
        "name".to_string(),
        JsonValue::String(model_name.to_string()),
    );

    let mut models = Map::new();
    models.insert(model_id.to_string(), JsonValue::Object(model_details));

    let mut provider_block = Map::new();
    provider_block.insert(
        "npm".to_string(),
        JsonValue::String(npm_package.to_string()),
    );
    provider_block.insert(
        "name".to_string(),
        JsonValue::String(provider_name.to_string()),
    );
    provider_block.insert("options".to_string(), JsonValue::Object(options));
    provider_block.insert("models".to_string(), JsonValue::Object(models));
    provider_block
}

fn parse_existing_config(existing: &str) -> Result<Map<String, JsonValue>, AppError> {
    if existing.trim().is_empty() {
        return Ok(Map::new());
    }

    let normalized = normalize_jsonc(existing)?;
    let value: JsonValue =
        serde_json::from_str(&normalized).map_err(|error| AppError::Validation {
            code: "validation.opencode_config_json",
            message: "Existing OpenCode config is not valid JSON or JSONC".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    value
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::Validation {
            code: "validation.opencode_config_json",
            message: "Existing OpenCode config must be a JSON object".to_string(),
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
    nested_string(target_options, "opencode", "model")
        .or_else(|| string_at(target_options, "model"))
        .or_else(|| nested_string(model_config, "opencode", "model"))
        .or_else(|| string_at(model_config, "default"))
        .or_else(|| string_at(model_config, "model"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Validation {
            code: "validation.provider_model_required",
            message: "OpenCode real switching requires a model id".to_string(),
            details: Some(
                "Set model_config_json.default or target_options_json.opencode.model".to_string(),
            ),
            recoverable: true,
        })
}

fn resolve_env_key(target_options: &JsonValue) -> String {
    nested_string(target_options, "opencode", "env_key")
        .or_else(|| string_at(target_options, "env_key"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_ENV_KEY.to_string())
}

fn resolve_npm_package(target_options: &JsonValue) -> String {
    nested_string(target_options, "opencode", "npm")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_NPM_PACKAGE.to_string())
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

fn opencode_provider_slug(provider_id: &str) -> String {
    let mut safe = String::new();
    let mut previous_was_separator = false;

    for character in provider_id.chars().flat_map(char::to_lowercase) {
        let next = if character.is_ascii_alphanumeric() {
            character
        } else {
            '-'
        };

        if next == '-' {
            if !previous_was_separator {
                safe.push(next);
            }
            previous_was_separator = true;
        } else {
            safe.push(next);
            previous_was_separator = false;
        }
    }

    let safe = safe.trim_matches('-');
    if safe.is_empty() {
        "ai-switch-provider".to_string()
    } else {
        format!("ai-switch-{safe}")
    }
}

fn normalize_jsonc(input: &str) -> Result<String, AppError> {
    let without_comments = strip_jsonc_comments(input)?;
    Ok(remove_trailing_commas(&without_comments))
}

fn strip_jsonc_comments(input: &str) -> Result<String, AppError> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(character) = chars.next() {
        if in_line_comment {
            if character == '\n' {
                in_line_comment = false;
                output.push(character);
            }
            continue;
        }

        if in_block_comment {
            if character == '\n' {
                output.push(character);
            }
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => {
                in_string = true;
                output.push(character);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                in_line_comment = true;
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block_comment = true;
            }
            _ => output.push(character),
        }
    }

    if in_block_comment {
        return Err(AppError::Validation {
            code: "validation.opencode_config_json",
            message: "Existing OpenCode config contains an unterminated block comment".to_string(),
            details: None,
            recoverable: true,
        });
    }

    Ok(output)
}

fn remove_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < chars.len() {
        let character = chars[index];

        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if character == '"' {
            in_string = true;
            output.push(character);
            index += 1;
            continue;
        }

        if character == ',' {
            let mut lookahead = index + 1;
            while lookahead < chars.len() && chars[lookahead].is_whitespace() {
                lookahead += 1;
            }
            if lookahead < chars.len() && (chars[lookahead] == '}' || chars[lookahead] == ']') {
                index += 1;
                continue;
            }
        }

        output.push(character);
        index += 1;
    }

    output
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
            model_config_json: "{\"default\":\"gpt-4.1\"}".to_string(),
            target_options_json:
                "{\"opencode\":{\"env_key\":\"ACME_API_KEY\",\"model\":\"gpt-4.1-mini\",\"model_name\":\"GPT 4.1 Mini\"}}"
                    .to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn resolves_custom_opencode_config_path() {
        let dir = tempdir().expect("tempdir");
        let custom_path = dir.path().join("custom-opencode.json");

        let path =
            resolve_opencode_config_path_with(Some(&custom_path), Path::new("C:/Users/example"))
                .expect("path");

        assert_eq!(path, custom_path);
    }

    #[test]
    fn renders_provider_config_and_preserves_unrelated_jsonc() {
        let existing = r#"
{
  // Keep unrelated settings.
  "autoupdate": false,
  "provider": {
    "other": {
      "name": "Other",
    },
  },
}
"#;

        let rendered = render_opencode_provider_config_from_str(
            Path::new("C:/Users/example/.config/opencode/opencode.json"),
            existing,
            &provider(),
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(rendered.provider_slug, "ai-switch-provider-1");
        assert_eq!(rendered.model_id, "gpt-4.1-mini");
        assert_eq!(parsed["$schema"], OPENCODE_SCHEMA);
        assert_eq!(parsed["autoupdate"], false);
        assert_eq!(parsed["provider"]["other"]["name"], "Other");
        assert_eq!(parsed["model"], "ai-switch-provider-1/gpt-4.1-mini");
        assert_eq!(
            parsed["provider"]["ai-switch-provider-1"]["npm"],
            DEFAULT_NPM_PACKAGE
        );
        assert_eq!(
            parsed["provider"]["ai-switch-provider-1"]["options"]["baseURL"],
            "https://api.example.com/v1"
        );
        assert_eq!(
            parsed["provider"]["ai-switch-provider-1"]["options"]["apiKey"],
            "{env:ACME_API_KEY}"
        );
        assert_eq!(
            parsed["provider"]["ai-switch-provider-1"]["models"]["gpt-4.1-mini"]["name"],
            "GPT 4.1 Mini"
        );
        assert!(rendered.contents.contains("{env:ACME_API_KEY}"));
        assert!(!rendered.contents.contains("secret://provider/acme"));
    }

    #[test]
    fn falls_back_to_model_config_default_and_default_env_key() {
        let mut provider = provider();
        provider.target_options_json = "{}".to_string();

        let rendered = render_opencode_provider_config_from_str(
            Path::new("C:/Users/example/.config/opencode/opencode.json"),
            "",
            &provider,
        )
        .expect("rendered");
        let parsed: Value = serde_json::from_str(&rendered.contents).expect("json");

        assert_eq!(parsed["model"], "ai-switch-provider-1/gpt-4.1");
        assert_eq!(
            parsed["provider"]["ai-switch-provider-1"]["options"]["apiKey"],
            "{env:OPENAI_API_KEY}"
        );
    }

    #[test]
    fn rejects_malformed_existing_json() {
        let error = render_opencode_provider_config_from_str(
            Path::new("C:/Users/example/.config/opencode/opencode.json"),
            "{\"model\":",
            &provider(),
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.opencode_config_json");
    }

    #[test]
    fn rejects_missing_base_url() {
        let mut provider = provider();
        provider.base_url = None;

        let error = render_opencode_provider_config_from_str(
            Path::new("C:/Users/example/.config/opencode/opencode.json"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_base_url_required");
    }

    #[test]
    fn rejects_missing_model_id() {
        let mut provider = provider();
        provider.model_config_json = "{}".to_string();
        provider.target_options_json = "{}".to_string();

        let error = render_opencode_provider_config_from_str(
            Path::new("C:/Users/example/.config/opencode/opencode.json"),
            "",
            &provider,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.provider_model_required");
    }
}
