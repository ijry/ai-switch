use super::{
    existing_text, generated_invalid, invalid_existing_config, ClientModel, RouteConfigInput,
    TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(super) struct ZCodeAdapter {
    target_key: &'static str,
    platform: PlatformId,
    /// Selects ZCode's wire protocol. `openai` hits `{baseURL}/responses`;
    /// `anthropic` hits `{baseURL}/v1/messages`.
    kind: &'static str,
    /// Appended to the proxy base URL. Differs per kind because ZCode adds its
    /// own suffix: anthropic would otherwise produce `/v1/v1/messages`.
    base_url_suffix: &'static str,
    /// Record key used when no existing entry can be adopted. Never prefixed
    /// with `builtin:` (coerced to a builtin and dropped from ZCode's registry)
    /// or `default-` (filtered when the key is blank).
    fallback_provider_id: &'static str,
    display_name: &'static str,
}

impl ZCodeAdapter {
    pub(super) const fn codex() -> Self {
        Self {
            target_key: "zcode_codex",
            platform: PlatformId::Codex,
            kind: "openai",
            base_url_suffix: "/v1",
            fallback_provider_id: "ai-switch-codex",
            display_name: "AI Switch (Codex)",
        }
    }

    pub(super) const fn claude() -> Self {
        Self {
            target_key: "zcode_claude",
            platform: PlatformId::Claude,
            kind: "anthropic",
            base_url_suffix: "",
            fallback_provider_id: "ai-switch-claude",
            display_name: "AI Switch (Claude)",
        }
    }

    fn base_url(&self, base_url: &str) -> String {
        let trimmed = base_url.trim().trim_end_matches('/');
        if self.base_url_suffix.is_empty() {
            return trimmed.to_string();
        }
        if trimmed
            .rsplit('/')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
        {
            return trimmed.to_string();
        }
        format!("{trimmed}{}", self.base_url_suffix)
    }

    /// Record key of the entry we should write into, in priority order: our own
    /// marker, then a hand-made entry recognizable by base URL plus current or
    /// rotated key.
    fn adoption_target(
        &self,
        providers: &Map<String, Value>,
        input: &RouteConfigInput,
    ) -> Option<String> {
        if let Some(key) = providers.iter().find_map(|(key, entry)| {
            let managed = entry.pointer("/aiSwitch/managed").and_then(Value::as_bool) == Some(true);
            let platform = entry
                .pointer("/aiSwitch/platform")
                .and_then(Value::as_str)
                .is_some_and(|value| value == self.platform.as_str());
            (managed && platform).then(|| key.clone())
        }) {
            return Some(key);
        }

        let expected_base = self.base_url(&input.base_url);
        providers.iter().find_map(|(key, entry)| {
            let base_matches = entry
                .pointer("/options/baseURL")
                .and_then(Value::as_str)
                .map(|value| value.trim().trim_end_matches('/'))
                .is_some_and(|value| value == expected_base);
            let api_key = entry
                .pointer("/options/apiKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            let key_matches = !api_key.is_empty()
                && (api_key == input.route_proxy_key
                    || input
                        .route_proxy_key_aliases
                        .iter()
                        .any(|alias| alias == api_key));
            (base_matches && key_matches).then(|| key.clone())
        })
    }

    fn model_entries(&self, models: &[ClientModel]) -> Map<String, Value> {
        models
            .iter()
            .map(|model| {
                (
                    model.id.clone(),
                    json!({
                        "limit": {
                            "context": model.context_window,
                            "output": model.max_output_tokens,
                        },
                        "modalities": { "input": ["text"], "output": ["text"] },
                    }),
                )
            })
            .collect()
    }
}

impl TargetAdapter for ZCodeAdapter {
    fn target_key(&self) -> &'static str {
        self.target_key
    }

    fn client_key(&self) -> &'static str {
        "zcode"
    }

    fn client_display_name(&self) -> &'static str {
        "ZCode"
    }

    fn native(&self) -> bool {
        false
    }

    fn restart_required(&self) -> bool {
        true
    }

    fn requires_client_models(&self) -> bool {
        true
    }

    fn platform(&self) -> PlatformId {
        self.platform
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(".zcode").join("v2").join("config.json")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let mut config = match existing {
            Some(bytes) => {
                let content = existing_text(path, "JSON", bytes)?;
                if content.trim().is_empty() {
                    Value::Object(Map::new())
                } else {
                    serde_json::from_str(content)
                        .map_err(|_| invalid_existing_config(path, "JSON", "syntax is invalid"))?
                }
            }
            None => Value::Object(Map::new()),
        };

        let root = config
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "root value must be an object"))?;
        let providers = root
            .entry("provider".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "provider must be an object"))?;

        let provider_id = self
            .adoption_target(providers, input)
            .unwrap_or_else(|| self.fallback_provider_id.to_string());
        let existing_entry = providers.get(&provider_id).cloned();
        let entry = providers
            .entry(provider_id)
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                invalid_existing_config(path, "JSON", "provider entry must be an object")
            })?;

        // Keep a name the user may have edited; only fill one in when adopting an
        // entry that has none.
        let name = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(self.display_name)
            .to_string();
        entry.insert("name".to_string(), Value::String(name));
        entry.insert("kind".to_string(), Value::String(self.kind.to_string()));
        entry.insert("source".to_string(), Value::String("custom".to_string()));

        let mut options = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("options"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        options.insert(
            "apiKey".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );
        options.insert(
            "baseURL".to_string(),
            Value::String(self.base_url(&input.base_url)),
        );
        options.insert("apiKeyRequired".to_string(), Value::Bool(true));
        entry.insert("options".to_string(), Value::Object(options));

        entry.insert(
            "models".to_string(),
            Value::Object(self.model_entries(&input.client_models)),
        );
        entry.insert(
            "aiSwitch".to_string(),
            json!({ "managed": true, "platform": self.platform.as_str() }),
        );

        let rendered =
            serde_json::to_vec_pretty(&config).map_err(|_| generated_invalid(path, "JSON"))?;
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
        let Some(root) = config.as_object() else {
            return TargetInspection::invalid();
        };

        // Both ZCode adapters read this one file, so the marker has to be matched
        // per platform or each target would report the other's entry as its own.
        let managed = root
            .get("provider")
            .and_then(Value::as_object)
            .is_some_and(|providers| {
                providers.values().any(|entry| {
                    entry.pointer("/aiSwitch/managed").and_then(Value::as_bool) == Some(true)
                        && entry
                            .pointer("/aiSwitch/platform")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == self.platform.as_str())
                })
            });

        TargetInspection::valid(managed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{ClientModel, TargetAdapterRegistry};
    use serde_json::json;

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const CODEX_KEY: &str = "sk-ai-switch-codexkey";

    fn input(models: &[&str]) -> RouteConfigInput {
        RouteConfigInput {
            base_url: BASE_URL.to_string(),
            route_proxy_key: CODEX_KEY.to_string(),
            route_proxy_key_aliases: Vec::new(),
            claude_env: crate::adapters::route_config::ClaudeEnvPlan::default(),
            client_models: models
                .iter()
                .map(|id| ClientModel {
                    id: (*id).to_string(),
                    context_window: 200_000,
                    max_output_tokens: 128_000,
                })
                .collect(),
        }
    }

    fn codex_adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("zcode", PlatformId::Codex)
            .expect("zcode codex adapter")
    }

    fn claude_adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("zcode", PlatformId::Claude)
            .expect("zcode claude adapter")
    }

    fn render(adapter: &dyn TargetAdapter, existing: Option<&[u8]>, models: &[&str]) -> Value {
        let bytes = adapter
            .render(Path::new("config.json"), existing, &input(models))
            .expect("render");
        serde_json::from_slice(&bytes).expect("valid JSON")
    }
    #[test]
    fn adapter_identity_declares_zcode_as_a_restart_required_non_native_client() {
        for adapter in [codex_adapter(), claude_adapter()] {
            assert_eq!(adapter.client_key(), "zcode");
            assert!(
                !adapter.native(),
                "ZCode is not a platform's first-party CLI"
            );
            // ZCode reads config at startup and has no file watcher.
            assert!(adapter.restart_required());
            // ZCode never probes /v1/models for custom providers.
            assert!(adapter.requires_client_models());
        }
        assert_eq!(codex_adapter().target_key(), "zcode_codex");
        assert_eq!(claude_adapter().target_key(), "zcode_claude");
    }

    #[test]
    fn resolved_path_is_the_desktop_provider_store() {
        let home = Path::new("/home/user");
        assert_eq!(
            codex_adapter().resolve_path(home),
            home.join(".zcode").join("v2").join("config.json")
        );
    }

    #[test]
    fn codex_writes_a_v1_suffixed_base_url_and_claude_does_not() {
        // kind=openai hits {baseURL}/responses, so the /v1 has to be in baseURL.
        let json = render(codex_adapter().as_ref(), None, &["gpt-5.6-sol"]);
        let entry = &json["provider"]["ai-switch-codex"];
        assert_eq!(entry["kind"], "openai");
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");

        // kind=anthropic appends /v1/messages itself; a /v1 here would produce
        // /v1/v1/messages.
        let json = render(claude_adapter().as_ref(), None, &["claude-sonnet-alias"]);
        let entry = &json["provider"]["ai-switch-claude"];
        assert_eq!(entry["kind"], "anthropic");
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527");
    }

    #[test]
    fn managed_entry_carries_credentials_models_and_the_managed_marker() {
        let json = render(codex_adapter().as_ref(), None, &["gpt-5.6-sol"]);
        let entry = &json["provider"]["ai-switch-codex"];

        assert_eq!(entry["options"]["apiKey"], CODEX_KEY);
        assert_eq!(entry["options"]["apiKeyRequired"], true);
        assert_eq!(entry["source"], "custom");
        assert_eq!(entry["aiSwitch"]["managed"], true);
        assert_eq!(entry["aiSwitch"]["platform"], "codex");
        assert_eq!(
            entry["models"]["gpt-5.6-sol"]["limit"],
            json!({ "context": 200000, "output": 128000 })
        );
        assert_eq!(
            entry["models"]["gpt-5.6-sol"]["modalities"],
            json!({ "input": ["text"], "output": ["text"] })
        );

        // apiFormat is recomputed from kind, so writing it would be misleading.
        assert!(entry.get("apiFormat").is_none());
        // Leaving enabled unset keeps the entry "not explicitly disabled".
        assert!(entry.get("enabled").is_none());
        // ZCode owns the per-model zcode sidecar and rewrites whatever we put there.
        assert!(entry["models"]["gpt-5.6-sol"].get("zcode").is_none());
        // Omitting name makes ZCode fall back to the record key.
        assert!(entry["models"]["gpt-5.6-sol"].get("name").is_none());
    }
    #[test]
    fn render_preserves_other_providers_and_the_sibling_platform_entry() {
        let existing = br#"{
  "$schema": "https://example.invalid/schema.json",
  "provider": {
    "builtin:bigmodel": {
      "name": "Bigmodel - API Key",
      "kind": "anthropic",
      "options": { "apiKey": "", "baseURL": "https://open.bigmodel.cn/api/anthropic" }
    },
    "ai-switch-claude": {
      "name": "AI Switch (Claude)",
      "kind": "anthropic",
      "options": { "apiKey": "sk-ai-switch-claudekey", "baseURL": "http://127.0.0.1:19527" },
      "aiSwitch": { "managed": true, "platform": "claude" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);

        // A corrupted write here empties every provider the user has, so the
        // untouched entries matter as much as the one we write.
        assert_eq!(
            json["provider"]["builtin:bigmodel"]["name"],
            "Bigmodel - API Key"
        );
        assert_eq!(
            json["provider"]["ai-switch-claude"]["options"]["apiKey"],
            "sk-ai-switch-claudekey"
        );
        assert_eq!(json["$schema"], "https://example.invalid/schema.json");
        assert_eq!(json["provider"]["ai-switch-codex"]["kind"], "openai");
    }

    #[test]
    fn adoption_claims_the_entry_marked_managed_for_this_platform() {
        let existing = br#"{
  "provider": {
    "3c109843-30ed-4307-a74e-ac537218d8be": {
      "name": "My Renamed Pool",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-stale", "baseURL": "http://127.0.0.1:1/v1" },
      "aiSwitch": { "managed": true, "platform": "codex" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let entry = &json["provider"]["3c109843-30ed-4307-a74e-ac537218d8be"];

        // Adopt in place: a second entry pointing at the same proxy would show up
        // twice in ZCode's picker.
        assert!(json["provider"].get("ai-switch-codex").is_none());
        // The user may have renamed it; renaming it back is a surprise.
        assert_eq!(entry["name"], "My Renamed Pool");
        assert_eq!(entry["options"]["apiKey"], CODEX_KEY);
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");
    }

    #[test]
    fn adoption_claims_a_hand_made_entry_by_base_url_and_key() {
        // What a user who wired this up by hand actually has: no managed marker,
        // but the platform's own sk and a local base URL.
        let existing = br#"{
  "provider": {
    "3c109843-30ed-4307-a74e-ac537218d8be": {
      "name": "Ai-Switch",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-codexkey", "baseURL": "http://127.0.0.1:19527/v1" },
      "models": { "glm-5.3": { "name": "GLM-5.3" } }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let entry = &json["provider"]["3c109843-30ed-4307-a74e-ac537218d8be"];

        assert!(json["provider"].get("ai-switch-codex").is_none());
        assert_eq!(entry["aiSwitch"]["managed"], true);
        // Confirmed with the user: models is replaced wholesale, so a hand-added
        // model is removed rather than merged.
        assert!(entry["models"].get("glm-5.3").is_none());
        assert_eq!(entry["models"]["gpt-5.6-sol"]["limit"]["context"], 200000);
    }

    #[test]
    fn adoption_recognizes_a_rotated_key_via_the_alias_list() {
        let existing = br#"{
  "provider": {
    "hand-made": {
      "name": "Ai-Switch",
      "kind": "openai",
      "options": { "apiKey": "sk-ai-switch-previous", "baseURL": "http://127.0.0.1:19527/v1" }
    }
  }
}"#;

        let mut with_alias = input(&["gpt-5.6-sol"]);
        with_alias.route_proxy_key_aliases = vec!["sk-ai-switch-previous".to_string()];
        let bytes = codex_adapter()
            .render(Path::new("config.json"), Some(existing), &with_alias)
            .expect("render");
        let json: Value = serde_json::from_slice(&bytes).expect("valid JSON");

        // The user rotated their sk; the stale entry is still theirs.
        assert!(json["provider"].get("ai-switch-codex").is_none());
        assert_eq!(
            json["provider"]["hand-made"]["options"]["apiKey"],
            CODEX_KEY
        );
    }

    #[test]
    fn unrelated_local_provider_is_not_adopted() {
        // Same host, different port and a foreign key: not ours.
        let existing = br#"{
  "provider": {
    "someone-elses-proxy": {
      "name": "Other Proxy",
      "kind": "openai",
      "options": { "apiKey": "sk-not-ours", "baseURL": "http://127.0.0.1:8080/v1" }
    }
  }
}"#;

        let json = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);

        assert_eq!(
            json["provider"]["someone-elses-proxy"]["options"]["apiKey"],
            "sk-not-ours"
        );
        assert_eq!(
            json["provider"]["ai-switch-codex"]["options"]["apiKey"],
            CODEX_KEY
        );
    }
    #[test]
    fn inspection_filters_by_platform_so_the_two_targets_do_not_impersonate_each_other() {
        let path = Path::new("config.json");
        assert_eq!(codex_adapter().inspect(path, None).file_status, "missing");

        let claude_only = br#"{
  "provider": {
    "ai-switch-claude": {
      "kind": "anthropic",
      "options": { "apiKey": "k", "baseURL": "http://127.0.0.1:19527" },
      "aiSwitch": { "managed": true, "platform": "claude" }
    }
  }
}"#;
        // Both adapters read the same file, so an unfiltered check would report
        // the Codex target as managed here.
        assert_eq!(
            codex_adapter().inspect(path, Some(claude_only)).file_status,
            "unmanaged"
        );
        assert_eq!(
            claude_adapter()
                .inspect(path, Some(claude_only))
                .file_status,
            "managed"
        );

        let managed = codex_adapter()
            .render(path, None, &input(&["gpt-5.6-sol"]))
            .unwrap();
        let inspection = codex_adapter().inspect(path, Some(&managed));
        assert_eq!(inspection.file_status, "managed");
        assert!(inspection.managed);
    }

    #[test]
    fn corrupt_config_is_refused_rather_than_overwritten() {
        let path = Path::new("config.json");
        // A failed parse makes ZCode fall back to legacy files and end up with an
        // empty provider list, so overwriting would destroy every provider.
        let error = codex_adapter()
            .render(path, Some(b"{not json"), &input(&["gpt-5.6-sol"]))
            .expect_err("must refuse");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_config_existing_invalid",
                ..
            }
        ));

        assert_eq!(
            codex_adapter()
                .inspect(path, Some(b"{not json"))
                .file_status,
            "invalid"
        );
        // A JSON array parses but has no place for `provider`.
        assert_eq!(
            codex_adapter().inspect(path, Some(b"[]")).file_status,
            "invalid"
        );
        assert!(codex_adapter()
            .render(path, Some(b"[]"), &input(&["gpt-5.6-sol"]))
            .is_err());
    }

    #[test]
    fn empty_config_file_renders_a_fresh_provider_map() {
        let json = render(codex_adapter().as_ref(), Some(b"   "), &["gpt-5.6-sol"]);
        assert_eq!(json["provider"]["ai-switch-codex"]["kind"], "openai");
    }
}
