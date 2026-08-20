mod codex;
mod json_agent;

pub(crate) use codex::codex_model_catalog_path;

use crate::{
    error::AppError,
    models::{platform::PlatformId, route_credential::ClaudeSlotWrite},
};
use codex::CodexAdapter;
use json_agent::JsonAgentAdapter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) const INVALID_EXISTING_CONFIG_CODE: &str = "validation.route_config_existing_invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConfigInput {
    pub base_url: String,
    pub route_proxy_key: String,
    /// Claude-only model env plan. Every field is a generic alias rather than an
    /// account's upstream model name: one settings file serves the whole pool,
    /// so the proxy does the per-account translation. An empty plan clears every
    /// managed key.
    pub claude_env: ClaudeEnvPlan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeEnvPlan {
    pub subagent_model: Option<String>,
    /// One entry per `CLAUDE_MODEL_SLOTS` slot, in that order. An empty vec (or
    /// a defaulted entry) clears the slot's keys.
    pub slots: Vec<ClaudeSlotWrite>,
    /// Pool-wide client behavior switches merged into the settings file's root.
    /// Authoritative: a key here overwrites a hand-edited value, and dropping a
    /// key from here removes it from the file (tracked via `aiSwitch.managedKeys`
    /// so we only ever remove keys we put there).
    pub client_config: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetInspection {
    pub file_status: String,
    pub managed: bool,
    pub error_code: Option<String>,
}

impl TargetInspection {
    pub(super) fn missing() -> Self {
        Self {
            file_status: "missing".to_string(),
            managed: false,
            error_code: None,
        }
    }

    pub(super) fn valid(managed: bool) -> Self {
        Self {
            file_status: if managed { "managed" } else { "unmanaged" }.to_string(),
            managed,
            error_code: None,
        }
    }

    pub(super) fn invalid() -> Self {
        Self {
            file_status: "invalid".to_string(),
            managed: false,
            error_code: Some(INVALID_EXISTING_CONFIG_CODE.to_string()),
        }
    }
}

pub trait TargetAdapter: Send + Sync {
    fn target_key(&self) -> &'static str;
    fn platform(&self) -> PlatformId;
    fn resolve_path(&self, home: &Path) -> PathBuf;
    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError>;
    fn inspect(&self, path: &Path, existing: Option<&[u8]>) -> TargetInspection;
}

#[derive(Clone)]
pub struct TargetAdapterRegistry {
    adapters: Vec<Arc<dyn TargetAdapter>>,
}

impl TargetAdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Arc::new(CodexAdapter),
                Arc::new(JsonAgentAdapter::claude()),
                Arc::new(JsonAgentAdapter::gemini()),
                Arc::new(JsonAgentAdapter::grok()),
            ],
        }
    }

    pub fn for_platform(&self, platform: PlatformId) -> Option<Arc<dyn TargetAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.platform() == platform)
            .cloned()
    }

    pub fn by_target_key(&self, target_key: &str) -> Option<Arc<dyn TargetAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.target_key() == target_key)
            .cloned()
    }
}

impl Default for TargetAdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn existing_text<'a>(
    path: &Path,
    format: &str,
    bytes: &'a [u8],
) -> Result<&'a str, AppError> {
    std::str::from_utf8(bytes)
        .map_err(|_| invalid_existing_config(path, format, "content is not valid UTF-8"))
}

pub(super) fn invalid_existing_config(path: &Path, format: &str, reason: &str) -> AppError {
    AppError::Validation {
        code: INVALID_EXISTING_CONFIG_CODE,
        message: "Existing CLI configuration is invalid; refusing to overwrite it".to_string(),
        details: Some(format!("{} ({format}): {reason}", path.display())),
        recoverable: true,
    }
}

pub(super) fn generated_invalid(path: &Path, format: &str) -> AppError {
    AppError::Validation {
        code: "config.generated_invalid",
        message: "Generated CLI configuration is invalid".to_string(),
        details: Some(format!("{} ({format})", path.display())),
        recoverable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::platform::PlatformId;
    use std::path::Path;

    const BASE_URL: &str = "http://127.0.0.1:43111";
    const ROUTE_PROXY_KEY: &str = "sk-ai-switch-test";

    fn input() -> RouteConfigInput {
        RouteConfigInput {
            base_url: BASE_URL.to_string(),
            route_proxy_key: ROUTE_PROXY_KEY.to_string(),
            claude_env: ClaudeEnvPlan::default(),
        }
    }

    #[test]
    fn registry_contains_only_verified_native_config_adapters() {
        let registry = TargetAdapterRegistry::new();

        assert_eq!(
            registry
                .for_platform(PlatformId::Codex)
                .unwrap()
                .target_key(),
            "codex"
        );
        assert_eq!(
            registry
                .for_platform(PlatformId::Claude)
                .unwrap()
                .target_key(),
            "claude_code"
        );
        assert_eq!(
            registry
                .for_platform(PlatformId::Gemini)
                .unwrap()
                .target_key(),
            "gemini_cli"
        );
        assert_eq!(
            registry
                .for_platform(PlatformId::Grok)
                .unwrap()
                .target_key(),
            "grok"
        );
        assert!(registry.for_platform(PlatformId::OpenCode).is_none());
        assert!(registry.for_platform(PlatformId::OpenClaw).is_none());
        assert!(registry.for_platform(PlatformId::Hermes).is_none());
        assert!(registry.by_target_key("claude_desktop").is_none());
    }

    #[test]
    fn codex_render_preserves_unmanaged_toml() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Codex).unwrap();
        let existing = br#"approval_policy = "never"

[model_providers.keep]
name = "Keep"
base_url = "https://keep.example/v1"
wire_api = "chat"
api_key_env_var = "KEEP_KEY"

[mcp_servers.filesystem]
command = "npx"
"#;

        let rendered = adapter
            .render(Path::new("config.toml"), Some(existing), &input())
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();

        assert!(rendered.contains("approval_policy = \"never\""));
        assert!(rendered.contains("[model_providers.keep]"));
        assert!(rendered.contains("api_key_env_var = \"KEEP_KEY\""));
        assert!(rendered.contains("[mcp_servers.filesystem]"));
        assert!(rendered.contains("model_provider = \"ai-switch\""));
        assert!(rendered.contains("model_catalog_json = \"ai-switch-model-catalog.json\""));
        assert!(rendered.contains("[model_providers.ai-switch]"));
        assert!(rendered.contains("base_url = \"http://127.0.0.1:43111/v1\""));
        assert!(rendered.contains("experimental_bearer_token = \"sk-ai-switch-test\""));
        assert!(!rendered.contains("api_key = \"sk-ai-switch-test\""));
    }

    #[test]
    fn codex_render_replaces_legacy_api_key() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Codex).unwrap();
        let existing = br#"model_provider = "ai-switch"

[model_providers.ai-switch]
name = "AI Switch Route Proxy"
base_url = "http://127.0.0.1:43111/v1"
wire_api = "responses"
api_key = "legacy-key"
"#;

        let rendered = adapter
            .render(Path::new("config.toml"), Some(existing), &input())
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();

        assert!(rendered.contains("model_catalog_json = \"ai-switch-model-catalog.json\""));
        assert!(rendered.contains("experimental_bearer_token = \"sk-ai-switch-test\""));
        assert!(!rendered.contains("api_key = \"legacy-key\""));
    }

    #[test]
    fn json_render_preserves_unmanaged_settings_and_env() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Claude).unwrap();
        let existing = br#"{
  "permissions": {
    "allow": ["Bash(ls)"]
  },
  "env": {
    "EXISTING_FLAG": "1",
    "ANTHROPIC_BASE_URL": "https://old.example"
  }
}"#;

        let rendered = adapter
            .render(Path::new("settings.json"), Some(existing), &input())
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&rendered).unwrap();

        assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], BASE_URL);
        assert_eq!(
            json["env"]["AI_SWITCH_ROUTE_PROXY_API_KEY"],
            ROUTE_PROXY_KEY
        );
        assert_eq!(json["aiSwitch"]["routeProxy"]["baseUrl"], BASE_URL);
        assert_eq!(json["aiSwitch"]["routeProxy"]["platform"], "claude");
    }

    #[test]
    fn claude_render_writes_and_clears_the_subagent_env_key() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Claude).unwrap();
        let existing = br#"{
  "includeCoAuthoredBy": false,
  "env": {
    "EXISTING_FLAG": "1"
  }
}"#;

        let with_alias = RouteConfigInput {
            claude_env: ClaudeEnvPlan {
                subagent_model: Some("claude-subagent".to_string()),
                slots: Vec::new(),
                client_config: None,
            },
            ..input()
        };
        let rendered = adapter
            .render(Path::new("settings.json"), Some(existing), &with_alias)
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&rendered).unwrap();

        // A generic alias, never an account's upstream model name: one settings
        // file serves the whole pool.
        assert_eq!(json["env"]["CLAUDE_CODE_SUBAGENT_MODEL"], "claude-subagent");
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(json["includeCoAuthoredBy"], false);

        // Mirror-inverse: rendering the same file with no alias removes the key
        // and leaves everything else alone.
        let cleared = adapter
            .render(Path::new("settings.json"), Some(&rendered), &input())
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&cleared).unwrap();

        assert!(json["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none());
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(json["includeCoAuthoredBy"], false);
        assert_eq!(json["aiSwitch"]["routeProxy"]["platform"], "claude");
    }

    #[test]
    fn claude_render_merges_and_removes_pool_wide_client_config() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Claude).unwrap();
        // The user hand-set includeCoAuthoredBy and owns hooks.
        let existing = br#"{
  "includeCoAuthoredBy": true,
  "hooks": {"PreToolUse": []},
  "env": {"EXISTING_FLAG": "1"}
}"#;

        let plan = |pairs: &[(&str, Value)]| RouteConfigInput {
            claude_env: ClaudeEnvPlan {
                client_config: (!pairs.is_empty()).then(|| {
                    pairs
                        .iter()
                        .map(|(key, value)| ((*key).to_string(), value.clone()))
                        .collect()
                }),
                ..ClaudeEnvPlan::default()
            },
            ..input()
        };

        let rendered = adapter
            .render(
                Path::new("settings.json"),
                Some(existing),
                &plan(&[
                    ("includeCoAuthoredBy", Value::Bool(false)),
                    ("cleanupPeriodDays", Value::from(30)),
                ]),
            )
            .unwrap();
        let json: Value = serde_json::from_slice(&rendered).unwrap();

        // Global config is authoritative: it overwrites the hand-set value.
        assert_eq!(json["includeCoAuthoredBy"], false);
        assert_eq!(json["cleanupPeriodDays"], 30);
        // Keys we never managed are untouched.
        assert_eq!(json["hooks"]["PreToolUse"], Value::Array(vec![]));
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(
            json["aiSwitch"]["managedClientKeys"],
            serde_json::json!(["cleanupPeriodDays", "includeCoAuthoredBy"])
        );

        // Dropping a key from the global config removes it from the file, while
        // the still-configured key stays.
        let narrowed = adapter
            .render(
                Path::new("settings.json"),
                Some(&rendered),
                &plan(&[("includeCoAuthoredBy", Value::Bool(false))]),
            )
            .unwrap();
        let json: Value = serde_json::from_slice(&narrowed).unwrap();

        assert!(json.get("cleanupPeriodDays").is_none());
        assert_eq!(json["includeCoAuthoredBy"], false);
        assert_eq!(json["hooks"]["PreToolUse"], Value::Array(vec![]));
        assert_eq!(
            json["aiSwitch"]["managedClientKeys"],
            serde_json::json!(["includeCoAuthoredBy"])
        );

        // Clearing the global config entirely removes every key we managed and
        // drops the bookkeeping — but never touches `hooks`, which we never wrote.
        let cleared = adapter
            .render(Path::new("settings.json"), Some(&narrowed), &plan(&[]))
            .unwrap();
        let json: Value = serde_json::from_slice(&cleared).unwrap();

        assert!(json.get("includeCoAuthoredBy").is_none());
        assert!(json["aiSwitch"].get("managedClientKeys").is_none());
        assert_eq!(json["hooks"]["PreToolUse"], Value::Array(vec![]));
    }

    #[test]
    fn client_config_never_removes_a_key_it_did_not_write() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Claude).unwrap();
        // The user set includeCoAuthoredBy by hand; we have never managed it, so
        // there is no managedClientKeys record for it.
        let existing = br#"{"includeCoAuthoredBy": true}"#;

        let cleared = adapter
            .render(Path::new("settings.json"), Some(existing), &input())
            .unwrap();
        let json: Value = serde_json::from_slice(&cleared).unwrap();

        assert_eq!(json["includeCoAuthoredBy"], true);
    }

    #[test]
    fn grok_render_never_writes_the_subagent_env_key() {
        let registry = TargetAdapterRegistry::new();
        let with_alias = RouteConfigInput {
            claude_env: ClaudeEnvPlan {
                subagent_model: Some("claude-subagent".to_string()),
                slots: Vec::new(),
                client_config: None,
            },
            ..input()
        };

        for platform in [PlatformId::Grok, PlatformId::Gemini] {
            let adapter = registry.for_platform(platform).unwrap();
            let rendered = adapter
                .render(Path::new("settings.json"), None, &with_alias)
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&rendered).unwrap();

            assert!(
                json["env"].get("CLAUDE_CODE_SUBAGENT_MODEL").is_none(),
                "platform={platform:?}"
            );
        }
    }

    #[test]
    fn codex_inspection_reports_missing_unmanaged_managed_and_invalid() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Codex).unwrap();
        let path = Path::new("config.toml");

        assert_eq!(adapter.inspect(path, None).file_status, "missing");
        assert_eq!(
            adapter
                .inspect(path, Some(br#"approval_policy = "never""#))
                .file_status,
            "unmanaged"
        );

        let managed = adapter.render(path, None, &input()).unwrap();
        let managed_inspection = adapter.inspect(path, Some(&managed));
        assert_eq!(managed_inspection.file_status, "managed");
        assert!(managed_inspection.managed);

        let invalid = adapter.inspect(path, Some(b"model_provider = [invalid"));
        assert_eq!(invalid.file_status, "invalid");
        assert!(!invalid.managed);
        assert_eq!(
            invalid.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
    }

    #[test]
    fn json_inspection_reports_missing_unmanaged_managed_and_invalid() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry.for_platform(PlatformId::Grok).unwrap();
        let path = Path::new("settings.json");

        assert_eq!(adapter.inspect(path, None).file_status, "missing");
        assert_eq!(
            adapter
                .inspect(path, Some(br#"{"theme":"dark"}"#))
                .file_status,
            "unmanaged"
        );

        let managed = adapter.render(path, None, &input()).unwrap();
        let managed_inspection = adapter.inspect(path, Some(&managed));
        assert_eq!(managed_inspection.file_status, "managed");
        assert!(managed_inspection.managed);

        let invalid = adapter.inspect(path, Some(b"{invalid"));
        assert_eq!(invalid.file_status, "invalid");
        assert!(!invalid.managed);
        assert_eq!(
            invalid.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
    }
}
