use super::{
    existing_text, generated_invalid, invalid_existing_config, RouteConfigInput, TargetAdapter,
    TargetInspection,
};
use crate::{
    error::AppError,
    models::{platform::PlatformId, route_credential::CLAUDE_MODEL_SLOTS},
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub(super) struct JsonAgentAdapter {
    target_key: &'static str,
    client_key: &'static str,
    client_display_name: &'static str,
    platform: PlatformId,
    config_dir: &'static str,
    platform_base_url_keys: &'static [&'static str],
    /// Env keys through which this agent supplies its credential. The route proxy
    /// key goes here so the agent actually authenticates against the local proxy —
    /// pointing it at `base_url` without a credential earns a 401 before any
    /// request reaches the pool.
    ///
    /// Empty for agents whose credential env key has not been verified; writing a
    /// guess would put a value the CLI may not read into the user's environment.
    platform_auth_token_keys: &'static [&'static str],
    /// Whether this agent honors Claude Code's model-slot env keys. Claude-only —
    /// gemini and grok have no equivalent, so they must never receive them.
    writes_claude_model_env: bool,
}

impl JsonAgentAdapter {
    pub(super) const fn claude() -> Self {
        Self {
            target_key: "claude_code",
            client_key: "claude_code",
            client_display_name: "Claude Code",
            platform: PlatformId::Claude,
            config_dir: ".claude",
            platform_base_url_keys: &["ANTHROPIC_BASE_URL"],
            platform_auth_token_keys: &["ANTHROPIC_AUTH_TOKEN"],
            writes_claude_model_env: true,
        }
    }

    pub(super) const fn gemini() -> Self {
        Self {
            target_key: "gemini_cli",
            client_key: "gemini_cli",
            client_display_name: "Gemini CLI",
            platform: PlatformId::Gemini,
            config_dir: ".gemini",
            platform_base_url_keys: &["GEMINI_API_BASE_URL", "GOOGLE_GEMINI_BASE_URL"],
            platform_auth_token_keys: &[],
            writes_claude_model_env: false,
        }
    }

    pub(super) const fn grok() -> Self {
        Self {
            target_key: "grok",
            client_key: "grok",
            client_display_name: "Grok",
            platform: PlatformId::Grok,
            config_dir: ".grok",
            platform_base_url_keys: &["XAI_API_BASE_URL", "GROK_API_BASE_URL"],
            platform_auth_token_keys: &[],
            writes_claude_model_env: false,
        }
    }
}

/// Env keys earlier versions wrote that nothing ever read.
///
/// `AI_SWITCH_ROUTE_PROXY` duplicated the platform's own base-url key, and
/// `AI_SWITCH_ROUTE_PROXY_API_KEY` duplicated `aiSwitch.routeProxy.apiKey` while
/// also exposing the credential to every process the agent spawns. Removed on
/// each write so they do not linger in configs written by those versions.
const LEGACY_UNREAD_ENV_KEYS: [&str; 2] =
    ["AI_SWITCH_ROUTE_PROXY", "AI_SWITCH_ROUTE_PROXY_API_KEY"];

/// Env key pinning the model for Claude Code's spawned subagents.
pub(super) const CLAUDE_SUBAGENT_MODEL_ENV_KEY: &str = "CLAUDE_CODE_SUBAGENT_MODEL";

/// Env key for requests that don't land on one of the four `/model` roles —
/// including Claude Code's own background subtasks.
pub(super) const CLAUDE_FALLBACK_MODEL_ENV_KEY: &str = "ANTHROPIC_MODEL";

fn set_or_remove(env: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            env.insert(key.to_string(), Value::String(value.to_string()));
        }
        None => {
            env.remove(key);
        }
    }
}

impl TargetAdapter for JsonAgentAdapter {
    fn target_key(&self) -> &'static str {
        self.target_key
    }

    fn client_key(&self) -> &'static str {
        self.client_key
    }

    fn client_display_name(&self) -> &'static str {
        self.client_display_name
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        false
    }

    fn requires_client_models(&self) -> bool {
        false
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

        // Read the previously-managed key list before we rewrite aiSwitch, so a
        // key dropped from the global config can be removed from the file.
        let previously_managed = root
            .get("aiSwitch")
            .and_then(|value| value.get("managedClientKeys"))
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

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
        // Without this the agent is told where the proxy is but not how to
        // authenticate to it, so every request is rejected before a credential is
        // even selected — which also means it never reaches the request log.
        for key in self.platform_auth_token_keys {
            env.insert(
                (*key).to_string(),
                Value::String(input.route_proxy_key.clone()),
            );
        }
        for key in LEGACY_UNREAD_ENV_KEYS {
            env.remove(key);
        }

        // Mirror-inverse on every managed key: write what the pool resolves,
        // remove what it doesn't, so a stale value never hardens into an
        // explicit setting. Claude-only — gemini/grok have no model slots.
        if self.writes_claude_model_env {
            set_or_remove(
                env,
                CLAUDE_SUBAGENT_MODEL_ENV_KEY,
                input.claude_env.subagent_model.as_deref(),
            );
            set_or_remove(
                env,
                CLAUDE_FALLBACK_MODEL_ENV_KEY,
                input.claude_env.fallback_model.as_deref(),
            );

            for (index, slot) in CLAUDE_MODEL_SLOTS.iter().enumerate() {
                let write = input.claude_env.slots.get(index);
                set_or_remove(
                    env,
                    slot.model_env_key,
                    write.and_then(|w| w.model.as_deref()),
                );
                set_or_remove(
                    env,
                    slot.name_env_key,
                    write.and_then(|w| w.display_name.as_deref()),
                );
            }
        }

        // Pool-wide client behavior switches, merged into the file's root. These
        // cannot be per-account: Claude Code reads them from its own settings
        // file, which the whole pool shares. The global config is authoritative,
        // and `managedClientKeys` records what we wrote so a key dropped from the
        // global config is removed rather than left orphaned — we never remove a
        // key we did not put there.
        if self.writes_claude_model_env {
            let managed = input.claude_env.client_config.clone().unwrap_or_default();

            for key in &previously_managed {
                if !managed.contains_key(key) {
                    root.remove(key);
                }
            }
            for (key, value) in &managed {
                root.insert(key.clone(), value.clone());
            }

            let ai_switch = object_entry(root, "aiSwitch", path)?;
            if managed.is_empty() {
                ai_switch.remove("managedClientKeys");
            } else {
                let mut keys = managed.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                ai_switch.insert(
                    "managedClientKeys".to_string(),
                    Value::Array(keys.into_iter().map(Value::String).collect()),
                );
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
    fn claude_model_env_is_claude_only() {
        assert!(JsonAgentAdapter::claude().writes_claude_model_env);
        assert!(!JsonAgentAdapter::gemini().writes_claude_model_env);
        assert!(!JsonAgentAdapter::grok().writes_claude_model_env);
    }

    #[test]
    fn only_claude_has_a_verified_auth_token_env_key() {
        // Claude Code reads ANTHROPIC_AUTH_TOKEN; writing the route proxy key
        // there is what lets it authenticate against the local proxy at all.
        assert_eq!(
            JsonAgentAdapter::claude().platform_auth_token_keys,
            &["ANTHROPIC_AUTH_TOKEN"]
        );
        // Left empty deliberately: guessing a credential env key would put a
        // value the CLI may not read into the user's environment. Fill these in
        // only once the real key is confirmed.
        assert!(JsonAgentAdapter::gemini()
            .platform_auth_token_keys
            .is_empty());
        assert!(JsonAgentAdapter::grok().platform_auth_token_keys.is_empty());
    }

    #[test]
    fn legacy_unread_env_keys_are_the_two_nothing_consumed() {
        assert_eq!(
            LEGACY_UNREAD_ENV_KEYS,
            ["AI_SWITCH_ROUTE_PROXY", "AI_SWITCH_ROUTE_PROXY_API_KEY"]
        );
    }
}
