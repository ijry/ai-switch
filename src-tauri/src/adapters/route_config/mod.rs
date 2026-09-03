mod codex;
mod deepseek_harness;
mod json_agent;
mod qoder_cli;
mod workbuddy;
mod zcode;

pub(crate) use codex::codex_model_catalog_path;

use crate::{
    error::AppError,
    models::{platform::PlatformId, route_credential::ClaudeSlotWrite},
};
use codex::CodexAdapter;
use deepseek_harness::DeepSeekHarnessAdapter;
use json_agent::JsonAgentAdapter;
use qoder_cli::QoderCliAdapter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use workbuddy::WorkBuddyAdapter;
use zcode::ZCodeAdapter;

pub(super) const INVALID_EXISTING_CONFIG_CODE: &str = "validation.route_config_existing_invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConfigInput {
    pub base_url: String,
    pub route_proxy_key: String,
    /// Proxy keys this platform used before rotation. A hand-made client entry
    /// still carrying an old key is the same user's entry, so adoption has to
    /// recognize it instead of adding a duplicate.
    pub route_proxy_key_aliases: Vec<String>,
    /// Claude-only model env plan. Every field is a generic alias rather than an
    /// account's upstream model name: one settings file serves the whole pool,
    /// so the proxy does the per-account translation. An empty plan clears every
    /// managed key.
    pub claude_env: ClaudeEnvPlan,
    /// Models the pool advertises, for clients that cannot discover models on
    /// their own. Empty for clients that can — the four native CLIs ignore it.
    pub client_models: Vec<ClientModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientModel {
    pub id: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientTargetDescriptor {
    pub client_key: String,
    pub display_name: String,
    /// This client is the platform's first-party CLI. Drives the dialog's
    /// default selection when the user has never chosen.
    pub native: bool,
    /// Long-running app that reads config at startup, so a write does not take
    /// effect until it restarts.
    pub restart_required: bool,
    /// Client cannot discover models on its own; the write must carry the pool's
    /// advertised model list.
    pub requires_client_models: bool,
    pub target_key: String,
    pub platform: PlatformId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeEnvPlan {
    pub subagent_model: Option<String>,
    /// Model for requests that don't land on one of the four `/model` roles.
    pub fallback_model: Option<String>,
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
    /// Client this adapter writes for. Distinct from `target_key`: one client
    /// can serve several platforms and then owns one target row per platform.
    fn client_key(&self) -> &'static str;
    fn client_display_name(&self) -> &'static str;
    /// Whether this client is the platform's first-party CLI.
    fn native(&self) -> bool;
    /// Whether the client must restart before a write takes effect.
    fn restart_required(&self) -> bool;
    /// Whether the client needs the pool's advertised model list written into
    /// its config because it cannot discover models itself.
    fn requires_client_models(&self) -> bool;
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
                Arc::new(JsonAgentAdapter::claude_desktop()),
                Arc::new(JsonAgentAdapter::gemini()),
                Arc::new(JsonAgentAdapter::grok()),
                Arc::new(ZCodeAdapter::codex()),
                Arc::new(ZCodeAdapter::claude()),
                Arc::new(DeepSeekHarnessAdapter::codex()),
                Arc::new(DeepSeekHarnessAdapter::claude()),
                Arc::new(WorkBuddyAdapter::codex()),
                Arc::new(WorkBuddyAdapter::claude()),
                Arc::new(WorkBuddyAdapter::codebuddy_codex()),
                Arc::new(WorkBuddyAdapter::codebuddy_claude()),
                Arc::new(QoderCliAdapter::codex()),
                Arc::new(QoderCliAdapter::claude()),
            ],
        }
    }

    pub fn by_client_and_platform(
        &self,
        client_key: &str,
        platform: PlatformId,
    ) -> Option<Arc<dyn TargetAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.client_key() == client_key && adapter.platform() == platform)
            .cloned()
    }

    pub fn clients_for_platform(&self, platform: PlatformId) -> Vec<ClientTargetDescriptor> {
        self.adapters
            .iter()
            .filter(|adapter| adapter.platform() == platform)
            .map(|adapter| ClientTargetDescriptor {
                client_key: adapter.client_key().to_string(),
                display_name: adapter.client_display_name().to_string(),
                native: adapter.native(),
                restart_required: adapter.restart_required(),
                requires_client_models: adapter.requires_client_models(),
                target_key: adapter.target_key().to_string(),
                platform: adapter.platform(),
            })
            .collect()
    }

    pub fn by_target_key(&self, target_key: &str) -> Option<Arc<dyn TargetAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.target_key() == target_key)
            .cloned()
    }

    /// Every registered adapter. Exists so tests outside this module can hold the
    /// registry and the `target_apps` seed table to each other.
    pub fn adapters(&self) -> &[Arc<dyn TargetAdapter>] {
        &self.adapters
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
            route_proxy_key_aliases: Vec::new(),
            claude_env: ClaudeEnvPlan::default(),
            client_models: Vec::new(),
        }
    }

    #[test]
    fn registry_contains_only_verified_native_config_adapters() {
        let registry = TargetAdapterRegistry::new();

        assert_eq!(
            registry
                .by_client_and_platform("codex", PlatformId::Codex)
                .unwrap()
                .target_key(),
            "codex"
        );
        assert_eq!(
            registry
                .by_client_and_platform("claude_code", PlatformId::Claude)
                .unwrap()
                .target_key(),
            "claude_code"
        );
        assert_eq!(
            registry
                .by_client_and_platform("gemini_cli", PlatformId::Gemini)
                .unwrap()
                .target_key(),
            "gemini_cli"
        );
        assert_eq!(
            registry
                .by_client_and_platform("grok", PlatformId::Grok)
                .unwrap()
                .target_key(),
            "grok"
        );
        assert!(registry
            .clients_for_platform(PlatformId::OpenCode)
            .is_empty());
        assert!(registry
            .clients_for_platform(PlatformId::OpenClaw)
            .is_empty());
        assert!(registry.clients_for_platform(PlatformId::Hermes).is_empty());
        assert!(registry.by_target_key("claude_desktop").is_some());
    }

    #[test]
    fn codex_render_preserves_unmanaged_toml() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry
            .by_client_and_platform("codex", PlatformId::Codex)
            .unwrap();
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
        let adapter = registry
            .by_client_and_platform("codex", PlatformId::Codex)
            .unwrap();
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
        let adapter = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
        // Carries both legacy keys so the render is also the migration path off
        // them.
        let existing = br#"{
  "permissions": {
    "allow": ["Bash(ls)"]
  },
  "env": {
    "EXISTING_FLAG": "1",
    "ANTHROPIC_BASE_URL": "https://old.example",
    "AI_SWITCH_ROUTE_PROXY": "https://old.example",
    "AI_SWITCH_ROUTE_PROXY_API_KEY": "sk-ai-switch-stale"
  }
}"#;

        let rendered = adapter
            .render(Path::new("settings.json"), Some(existing), &input())
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&rendered).unwrap();

        assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(json["env"]["EXISTING_FLAG"], "1");
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], BASE_URL);
        // Claude Code authenticates with this one. Without it the agent knows
        // where the proxy is but not how to authenticate, so it gets a 401.
        assert_eq!(json["env"]["ANTHROPIC_AUTH_TOKEN"], ROUTE_PROXY_KEY);
        // Nothing ever read these, and the second one leaked the credential into
        // every process the agent spawns.
        assert!(json["env"].get("AI_SWITCH_ROUTE_PROXY").is_none());
        assert!(json["env"].get("AI_SWITCH_ROUTE_PROXY_API_KEY").is_none());
        assert_eq!(json["aiSwitch"]["routeProxy"]["baseUrl"], BASE_URL);
        assert_eq!(json["aiSwitch"]["routeProxy"]["platform"], "claude");
        // The credential still travels here, outside `env`, so it is not handed
        // to spawned processes.
        assert_eq!(json["aiSwitch"]["routeProxy"]["apiKey"], ROUTE_PROXY_KEY);
    }

    #[test]
    fn claude_render_writes_and_clears_the_subagent_env_key() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
        let existing = br#"{
  "includeCoAuthoredBy": false,
  "env": {
    "EXISTING_FLAG": "1"
  }
}"#;

        let with_alias = RouteConfigInput {
            claude_env: ClaudeEnvPlan {
                subagent_model: Some("claude-subagent".to_string()),
                fallback_model: None,
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
        let adapter = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
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
        let adapter = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
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
                fallback_model: None,
                slots: Vec::new(),
                client_config: None,
            },
            ..input()
        };

        for (client_key, platform) in [
            ("grok", PlatformId::Grok),
            ("gemini_cli", PlatformId::Gemini),
        ] {
            let adapter = registry
                .by_client_and_platform(client_key, platform)
                .unwrap();
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
        let adapter = registry
            .by_client_and_platform("codex", PlatformId::Codex)
            .unwrap();
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
    fn registry_keys_are_unique_across_target_and_client_platform_pairs() {
        let registry = TargetAdapterRegistry::new();

        let mut target_keys = std::collections::HashSet::new();
        let mut client_platform_pairs = std::collections::HashSet::new();
        for adapter in &registry.adapters {
            assert!(
                target_keys.insert(adapter.target_key()),
                "duplicate target_key: {}",
                adapter.target_key()
            );
            assert!(
                client_platform_pairs.insert((adapter.client_key(), adapter.platform())),
                "duplicate (client_key, platform): {} {:?}",
                adapter.client_key(),
                adapter.platform()
            );
            assert!(
                !adapter.client_display_name().is_empty(),
                "empty display name: {}",
                adapter.target_key()
            );
        }
    }

    #[test]
    fn native_cli_adapters_resolve_by_client_and_platform() {
        let registry = TargetAdapterRegistry::new();

        for (client_key, platform, target_key) in [
            ("codex", PlatformId::Codex, "codex"),
            ("claude_code", PlatformId::Claude, "claude_code"),
            ("claude_desktop", PlatformId::Claude, "claude_desktop"),
            ("gemini_cli", PlatformId::Gemini, "gemini_cli"),
            ("grok", PlatformId::Grok, "grok"),
        ] {
            let adapter = registry
                .by_client_and_platform(client_key, platform)
                .unwrap_or_else(|| panic!("adapter for {client_key}"));
            assert_eq!(adapter.target_key(), target_key);
            assert!(adapter.native(), "{client_key} is a first-party CLI");
            // CLIs read config on next invocation, so nothing needs restarting.
            assert!(!adapter.restart_required(), "{client_key}");
        }

        // Wrong platform for a real client key resolves to nothing rather than
        // silently writing the wrong file.
        assert!(registry
            .by_client_and_platform("codex", PlatformId::Claude)
            .is_none());
        assert!(registry
            .by_client_and_platform("unknown", PlatformId::Codex)
            .is_none());
    }

    #[test]
    fn clients_for_platform_lists_the_native_cli_first_then_third_party_clients() {
        let registry = TargetAdapterRegistry::new();

        let codex = registry.clients_for_platform(PlatformId::Codex);
        assert_eq!(
            codex
                .iter()
                .map(|client| client.client_key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "codex",
                "zcode",
                "deepseek_harness",
                "workbuddy",
                "codebuddy_cli",
                "qoder_cli"
            ]
        );
        assert_eq!(codex[0].display_name, "Codex CLI");
        assert_eq!(codex[0].target_key, "codex");
        assert_eq!(codex[0].platform, PlatformId::Codex);
        // Every third-party client here reads its config at startup, so the
        // dialog has to say "restart" for all of them.
        for client in &codex[1..] {
            assert!(client.restart_required, "{}", client.client_key);
            assert!(!client.native, "{}", client.client_key);
            // None of them probe /v1/models, so the write must carry the list.
            assert!(client.requires_client_models, "{}", client.client_key);
        }

        // Claude platform should list both CLI and Desktop
        let claude = registry.clients_for_platform(PlatformId::Claude);
        assert_eq!(
            claude
                .iter()
                .map(|client| client.client_key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "claude_code",
                "claude_desktop",
                "zcode",
                "deepseek_harness",
                "workbuddy",
                "codebuddy_cli",
                "qoder_cli"
            ]
        );
        assert_eq!(claude[0].display_name, "Claude Code");
        assert_eq!(claude[1].display_name, "Claude Desktop");
        assert!(claude[0].native);
        assert!(claude[1].native);
        assert!(!claude[0].restart_required);
        assert!(!claude[1].restart_required);

        // Platforms with no adapter list nothing rather than erroring.
        assert!(registry.clients_for_platform(PlatformId::Hermes).is_empty());
    }

    #[test]
    fn client_models_carry_context_limits_and_are_ignored_by_native_adapters() {
        let registry = TargetAdapterRegistry::new();
        let with_models = RouteConfigInput {
            client_models: vec![
                ClientModel {
                    id: "gpt-5.6-sol".to_string(),
                    context_window: 200_000,
                    max_output_tokens: 128_000,
                },
                ClientModel {
                    id: "claude-sonnet-alias[1m]".to_string(),
                    context_window: 1_000_000,
                    max_output_tokens: 128_000,
                },
            ],
            ..input()
        };

        // The four native CLIs discover models themselves, so the list must not
        // leak into their files.
        let codex = registry
            .by_client_and_platform("codex", PlatformId::Codex)
            .unwrap();
        let rendered = codex
            .render(Path::new("config.toml"), None, &with_models)
            .unwrap();
        let rendered = String::from_utf8(rendered).unwrap();
        assert!(!rendered.contains("gpt-5.6-sol"));

        let claude = registry
            .by_client_and_platform("claude_code", PlatformId::Claude)
            .unwrap();
        let rendered = claude
            .render(Path::new("settings.json"), None, &with_models)
            .unwrap();
        assert!(!String::from_utf8(rendered).unwrap().contains("gpt-5.6-sol"));
    }

    #[test]
    fn json_inspection_reports_missing_unmanaged_managed_and_invalid() {
        let registry = TargetAdapterRegistry::new();
        let adapter = registry
            .by_client_and_platform("grok", PlatformId::Grok)
            .unwrap();
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
