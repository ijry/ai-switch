use super::{
    existing_text, generated_invalid, invalid_existing_config, RouteConfigInput, TargetAdapter,
    TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub(super) struct JsonAgentAdapter {
    target_key: &'static str,
    platform: PlatformId,
    config_dir: &'static str,
    platform_base_url_keys: &'static [&'static str],
    /// Env key that pins the model for agent-spawned subagents. Claude-only —
    /// gemini and grok have no equivalent, so they must never receive it.
    subagent_env_key: Option<&'static str>,
}

impl JsonAgentAdapter {
    pub(super) const fn claude() -> Self {
        Self {
            target_key: "claude_code",
            platform: PlatformId::Claude,
            config_dir: ".claude",
            platform_base_url_keys: &["ANTHROPIC_BASE_URL"],
            subagent_env_key: Some("CLAUDE_CODE_SUBAGENT_MODEL"),
        }
    }

    pub(super) const fn gemini() -> Self {
        Self {
            target_key: "gemini_cli",
            platform: PlatformId::Gemini,
            config_dir: ".gemini",
            platform_base_url_keys: &["GEMINI_API_BASE_URL", "GOOGLE_GEMINI_BASE_URL"],
            subagent_env_key: None,
        }
    }

    pub(super) const fn grok() -> Self {
        Self {
            target_key: "grok",
            platform: PlatformId::Grok,
            config_dir: ".grok",
            platform_base_url_keys: &["XAI_API_BASE_URL", "GROK_API_BASE_URL"],
            subagent_env_key: None,
        }
    }
}

impl TargetAdapter for JsonAgentAdapter {
    fn target_key(&self) -> &'static str {
        self.target_key
    }

    fn platform(&self) -> PlatformId {
        self.platform
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(self.config_dir).join("settings.json")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let (mut config, pretty) = match existing {
            Some(bytes) => {
                let content = existing_text(path, "JSON", bytes)?;
                if content.trim().is_empty() {
                    (Value::Object(Map::new()), false)
                } else {
                    (
                        serde_json::from_str(content).map_err(|_| {
                            invalid_existing_config(path, "JSON", "syntax is invalid")
                        })?,
                        true,
                    )
                }
            }
            None => (Value::Object(Map::new()), false),
        };

        let root = config
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "root value must be an object"))?;
        let ai_switch = object_entry(root, "aiSwitch", path)?;
        let route_proxy = object_entry(ai_switch, "routeProxy", path)?;
        route_proxy.insert("enabled".to_string(), Value::Bool(true));
        route_proxy.insert("baseUrl".to_string(), Value::String(input.base_url.clone()));
        route_proxy.insert(
            "platform".to_string(),
            Value::String(self.platform.as_str().to_string()),
        );
        route_proxy.insert(
            "apiKey".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );

        let env = object_entry(root, "env", path)?;
        for key in self.platform_base_url_keys {
            env.insert((*key).to_string(), Value::String(input.base_url.clone()));
        }
        env.insert(
            "AI_SWITCH_ROUTE_PROXY".to_string(),
            Value::String(input.base_url.clone()),
        );
        env.insert(
            "AI_SWITCH_ROUTE_PROXY_API_KEY".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );

        // Mirror-inverse: write the alias when the pool provides one, remove it
        // otherwise, so a stale value never hardens into an explicit setting.
        if let Some(key) = self.subagent_env_key {
            match input
                .subagent_model
                .as_deref()
                .map(str::trim)
                .filter(|alias| !alias.is_empty())
            {
                Some(alias) => {
                    env.insert(key.to_string(), Value::String(alias.to_string()));
                }
                None => {
                    env.remove(key);
                }
            }
        }

        let rendered = if pretty {
            serde_json::to_vec_pretty(&config)
        } else {
            serde_json::to_vec(&config)
        }
        .map_err(|_| generated_invalid(path, "JSON"))?;
        let generated: Value =
            serde_json::from_slice(&rendered).map_err(|_| generated_invalid(path, "JSON"))?;
        if !generated.is_object() {
            return Err(generated_invalid(path, "JSON"));
        }
        Ok(rendered)
    }

    fn inspect(&self, _path: &Path, existing: Option<&[u8]>) -> TargetInspection {
        let Some(bytes) = existing else {
            return TargetInspection::missing();
        };
        let Ok(config) = serde_json::from_slice::<Value>(bytes) else {
            return TargetInspection::invalid();
        };
        if !config.is_object() {
            return TargetInspection::invalid();
        }

        TargetInspection::valid(
            config
                .pointer("/aiSwitch/routeProxy/enabled")
                .and_then(Value::as_bool)
                == Some(true),
        )
    }
}

fn object_entry<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<&'a mut Map<String, Value>, AppError> {
    let value = parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    value
        .as_object_mut()
        .ok_or_else(|| invalid_existing_config(path, "JSON", &format!("{key} must be an object")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_specific_base_url_keys_are_explicit() {
        assert_eq!(
            JsonAgentAdapter::claude().platform_base_url_keys,
            &["ANTHROPIC_BASE_URL"]
        );
        assert_eq!(
            JsonAgentAdapter::gemini().platform_base_url_keys,
            &["GEMINI_API_BASE_URL", "GOOGLE_GEMINI_BASE_URL"]
        );
        assert_eq!(
            JsonAgentAdapter::grok().platform_base_url_keys,
            &["XAI_API_BASE_URL", "GROK_API_BASE_URL"]
        );
    }

    #[test]
    fn subagent_env_key_is_claude_only() {
        assert_eq!(
            JsonAgentAdapter::claude().subagent_env_key,
            Some("CLAUDE_CODE_SUBAGENT_MODEL")
        );
        assert_eq!(JsonAgentAdapter::gemini().subagent_env_key, None);
        assert_eq!(JsonAgentAdapter::grok().subagent_env_key, None);
    }
}
