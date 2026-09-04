use super::{
    existing_text, generated_invalid, invalid_existing_config, ClientModel, RouteConfigInput,
    TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_yaml::{Mapping, Value};
use std::path::{Path, PathBuf};

pub(super) struct DeepSeekHarnessAdapter {
    platform: PlatformId,
    /// Provider ID used in the yaml file
    provider_id: &'static str,
    display_name: &'static str,
}

impl DeepSeekHarnessAdapter {
    pub(super) const fn codex() -> Self {
        Self {
            platform: PlatformId::Codex,
            provider_id: "ai-switch-codex",
            display_name: "AI Switch (Codex)",
        }
    }

    pub(super) const fn claude() -> Self {
        Self {
            platform: PlatformId::Claude,
            provider_id: "ai-switch-claude",
            display_name: "AI Switch (Claude)",
        }
    }

    fn base_url(&self, base_url: &str) -> String {
        let trimmed = base_url.trim().trim_end_matches('/');
        // Always append /v1 for OpenAI-compatible API
        if trimmed
            .rsplit('/')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
        {
            return trimmed.to_string();
        }
        format!("{trimmed}/v1")
    }

    /// Which platform, if any, has claimed this provider entry.
    fn claimed_platform(entry: &Value) -> Option<&str> {
        entry
            .get("aiSwitch")
            .and_then(|marker| marker.get("platform"))
            .and_then(Value::as_str)
    }

    /// Which provider entry to write into: ours by marker, else an entry that
    /// already points at this proxy and no sibling platform has claimed.
    fn adoption_target(&self, providers: &Mapping, input: &RouteConfigInput) -> Option<String> {
        // First check for our managed marker
        if let Some(key) = providers.iter().find_map(|(key, entry)| {
            let managed = entry
                .get("aiSwitch")
                .and_then(|v| v.get("managed"))
                .and_then(Value::as_bool)
                == Some(true);
            let platform =
                Self::claimed_platform(entry).is_some_and(|value| value == self.platform.as_str());
            (managed && platform).then(|| key.as_str().map(str::to_string))?
        }) {
            return Some(key);
        }

        // Fall back to an entry already aimed at this proxy. Both platforms render
        // the same `/v1` base URL — unlike zcode, an OpenAI-compatible client has
        // no per-platform suffix to tell them apart — and `apiKeyEnv` is a constant
        // this adapter writes itself, so neither can disambiguate. Without the
        // platform check below, writing claude after codex adopted the codex entry
        // and rewrote it, `ai-switch-claude` never appeared, and the next codex
        // write flipped it back: the two platforms could not coexist and every
        // write silently destroyed the other one's provider.
        let expected_base = self.base_url(&input.base_url);
        providers.iter().find_map(|(key, entry)| {
            let base_matches = entry
                .get("baseURL")
                .and_then(Value::as_str)
                .map(|value| value.trim().trim_end_matches('/'))
                .is_some_and(|value| value == expected_base);
            // An unmarked entry is fair game — the user aimed it here, and the
            // write stamps it, so the sibling platform skips it from then on.
            let claimed_by_sibling =
                Self::claimed_platform(entry).is_some_and(|value| value != self.platform.as_str());
            (base_matches && !claimed_by_sibling).then(|| key.as_str().map(str::to_string))?
        })
    }

    /// One `models[]` entry per advertised model.
    ///
    /// `contextWindow` is the harness's own key for a model's capacity
    /// (`@deepseek-ai/dsh-llm-pi-ai`, `modelFields`), and it is worth writing
    /// because the harness has no other way to learn it: our route is a
    /// hand-declared provider, so nothing in its installed catalog describes
    /// these ids and every model would silently fall back to the route-level
    /// `defaultContextWindow`. A pool serving 1M would then be compacted at 256K
    /// here while the Codex CLI, which reads the same numbers from the catalog
    /// endpoint, used the full window.
    ///
    /// `maxTokens` is deliberately left out. It reads like the sibling of
    /// ZCode's `limit.output`, but the harness treats an explicitly configured
    /// `maxTokens` as that model's per-request output cap rather than as
    /// metadata — writing our generic 128K would change what goes out on the
    /// wire for every relay, including ones that cannot honour it.
    fn model_entries(&self, models: &[ClientModel]) -> Vec<Value> {
        models
            .iter()
            .map(|model| {
                let mut entry = Mapping::new();
                entry.insert(
                    Value::String("id".to_string()),
                    Value::String(model.id.clone()),
                );
                entry.insert(
                    Value::String("contextWindow".to_string()),
                    Value::Number(model.context_window.into()),
                );
                Value::Mapping(entry)
            })
            .collect()
    }
}

impl TargetAdapter for DeepSeekHarnessAdapter {
    fn target_key(&self) -> &'static str {
        match self.platform {
            PlatformId::Codex => "deepseek_harness_codex",
            PlatformId::Claude => "deepseek_harness_claude",
            _ => unreachable!(),
        }
    }

    fn client_key(&self) -> &'static str {
        "deepseek_harness"
    }

    fn client_display_name(&self) -> &'static str {
        "DeepSeek Harness"
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
        home.join(".dsh").join("settings.yaml")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let mut config = match existing {
            Some(bytes) => {
                let content = existing_text(path, "YAML", bytes)?;
                if content.trim().is_empty() {
                    Value::Mapping(Mapping::new())
                } else {
                    serde_yaml::from_str(content)
                        .map_err(|_| invalid_existing_config(path, "YAML", "syntax is invalid"))?
                }
            }
            None => Value::Mapping(Mapping::new()),
        };

        let root = config
            .as_mapping_mut()
            .ok_or_else(|| invalid_existing_config(path, "YAML", "root value must be a mapping"))?;

        // Ensure llm-pi-ai section exists
        if !root.contains_key(&Value::String("llm-pi-ai".to_string())) {
            root.insert(
                Value::String("llm-pi-ai".to_string()),
                Value::Mapping(Mapping::new()),
            );
        }
        let llm_pi_ai = root
            .get_mut(&Value::String("llm-pi-ai".to_string()))
            .and_then(Value::as_mapping_mut)
            .ok_or_else(|| invalid_existing_config(path, "YAML", "llm-pi-ai must be a mapping"))?;

        // Ensure providers section exists
        if !llm_pi_ai.contains_key(&Value::String("providers".to_string())) {
            llm_pi_ai.insert(
                Value::String("providers".to_string()),
                Value::Mapping(Mapping::new()),
            );
        }
        let providers = llm_pi_ai
            .get_mut(&Value::String("providers".to_string()))
            .and_then(Value::as_mapping_mut)
            .ok_or_else(|| {
                invalid_existing_config(path, "YAML", "llm-pi-ai.providers must be a mapping")
            })?;

        let provider_id = self
            .adoption_target(providers, input)
            .unwrap_or_else(|| self.provider_id.to_string());

        // Keep existing entry if present to preserve custom fields
        let existing_entry = providers.get(&Value::String(provider_id.clone())).cloned();

        // Create or update the provider entry
        let mut provider_entry = existing_entry
            .and_then(|v| v.as_mapping().cloned())
            .unwrap_or_else(Mapping::new);

        // Keep user-edited displayName if present
        let display_name = provider_entry
            .get(&Value::String("displayName".to_string()))
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(self.display_name)
            .to_string();

        provider_entry.insert(
            Value::String("displayName".to_string()),
            Value::String(display_name),
        );
        // The route authenticates with an `Authorization` header rather than
        // `apiKeyEnv`.
        //
        // `apiKeyEnv` is not a literal key: it is a *credential reference*, an
        // env-var-shaped name the harness resolves per request across its `env`,
        // `file` (`~/.dsh/.credentials.yaml` `refs:`), `project-env` and
        // `user-env` layers. Writing `apiKeyEnv: AI_SWITCH_API_KEY` while nothing
        // supplies that name leaves a dangling reference, and a named-but-
        // unresolvable reference fails the request with `MISSING_CREDENTIAL`
        // before any header is tried — so the old entry could never authenticate.
        //
        // A route naming no credential is deliberately unauthenticated and hands
        // the requirement to the protocol, and pi-ai's OpenAI-compatible
        // implementation accepts an `Authorization` header of its own. Profile
        // headers reach the request with only `user-agent` reserved for the
        // harness's own attribution, so this is the one mechanism that needs
        // neither an exported variable nor a write into the harness credential
        // store.
        provider_entry.remove(&Value::String("apiKeyEnv".to_string()));
        let mut headers = provider_entry
            .get(&Value::String("headers".to_string()))
            .and_then(Value::as_mapping)
            .cloned()
            .unwrap_or_else(Mapping::new);
        headers.insert(
            Value::String("Authorization".to_string()),
            Value::String(format!("Bearer {}", input.route_proxy_key)),
        );
        provider_entry.insert(
            Value::String("headers".to_string()),
            Value::Mapping(headers),
        );
        provider_entry.insert(
            Value::String("api".to_string()),
            Value::String("openai-completions".to_string()),
        );
        provider_entry.insert(
            Value::String("baseURL".to_string()),
            Value::String(self.base_url(&input.base_url)),
        );

        // Add models
        let model_list = Value::Sequence(self.model_entries(&input.client_models));
        provider_entry.insert(Value::String("models".to_string()), model_list);

        // Add managed marker
        let mut ai_switch = Mapping::new();
        ai_switch.insert(Value::String("managed".to_string()), Value::Bool(true));
        ai_switch.insert(
            Value::String("platform".to_string()),
            Value::String(self.platform.as_str().to_string()),
        );
        provider_entry.insert(
            Value::String("aiSwitch".to_string()),
            Value::Mapping(ai_switch),
        );

        providers.insert(Value::String(provider_id), Value::Mapping(provider_entry));

        let rendered =
            serde_yaml::to_string(&config).map_err(|_| generated_invalid(path, "YAML"))?;
        let _validated: Value =
            serde_yaml::from_str(&rendered).map_err(|_| generated_invalid(path, "YAML"))?;
        Ok(rendered.into_bytes())
    }

    fn inspect(&self, _path: &Path, existing: Option<&[u8]>) -> TargetInspection {
        let Some(bytes) = existing else {
            return TargetInspection::missing();
        };
        let Ok(content) = std::str::from_utf8(bytes) else {
            return TargetInspection::invalid();
        };
        let Ok(config) = serde_yaml::from_str::<Value>(content) else {
            return TargetInspection::invalid();
        };
        let Some(root) = config.as_mapping() else {
            return TargetInspection::invalid();
        };

        // Check for managed marker with matching platform
        let managed = root
            .get(&Value::String("llm-pi-ai".to_string()))
            .and_then(Value::as_mapping)
            .and_then(|llm| llm.get(&Value::String("providers".to_string())))
            .and_then(Value::as_mapping)
            .is_some_and(|providers| {
                providers.values().any(|entry| {
                    entry
                        .get("aiSwitch")
                        .and_then(|v| v.get("managed"))
                        .and_then(Value::as_bool)
                        == Some(true)
                        && entry
                            .get("aiSwitch")
                            .and_then(|v| v.get("platform"))
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

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const PROXY_KEY: &str = "sk-ai-switch-test";

    fn input(models: &[&str]) -> RouteConfigInput {
        RouteConfigInput {
            base_url: BASE_URL.to_string(),
            route_proxy_key: PROXY_KEY.to_string(),
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
            .by_client_and_platform("deepseek_harness", PlatformId::Codex)
            .expect("deepseek_harness codex adapter")
    }

    fn claude_adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("deepseek_harness", PlatformId::Claude)
            .expect("deepseek_harness claude adapter")
    }

    fn render(adapter: &dyn TargetAdapter, existing: Option<&[u8]>, models: &[&str]) -> Value {
        let bytes = adapter
            .render(Path::new("settings.yaml"), existing, &input(models))
            .expect("render");
        serde_yaml::from_slice(&bytes).expect("valid YAML")
    }

    #[test]
    fn both_platforms_coexist_instead_of_overwriting_each_other() {
        // Both platforms render the same `/v1` base URL, so before the adoption
        // fallback checked the platform marker, writing claude adopted the codex
        // entry and rewrote it in place: `ai-switch-claude` never appeared, and the
        // next codex write flipped it back. Every write destroyed the other
        // platform's provider.
        let codex = render(codex_adapter().as_ref(), None, &["gpt-5.5"]);
        let codex_bytes = serde_yaml::to_string(&codex).expect("codex yaml");
        let both = render(
            claude_adapter().as_ref(),
            Some(codex_bytes.as_bytes()),
            &["claude-opus-5"],
        );

        let providers = both["llm-pi-ai"]["providers"]
            .as_mapping()
            .expect("providers");
        assert!(
            providers.contains_key(Value::String("ai-switch-codex".into())),
            "codex provider must survive a claude write: {both:?}"
        );
        assert!(
            providers.contains_key(Value::String("ai-switch-claude".into())),
            "claude must get its own provider: {both:?}"
        );
        assert_eq!(
            both["llm-pi-ai"]["providers"]["ai-switch-codex"]["aiSwitch"]["platform"],
            "codex"
        );
        assert_eq!(
            both["llm-pi-ai"]["providers"]["ai-switch-codex"]["models"][0]["id"],
            "gpt-5.5"
        );
        assert_eq!(
            both["llm-pi-ai"]["providers"]["ai-switch-claude"]["models"][0]["id"],
            "claude-opus-5"
        );

        // And writing codex again must not disturb claude.
        let both_bytes = serde_yaml::to_string(&both).expect("both yaml");
        let again = render(
            codex_adapter().as_ref(),
            Some(both_bytes.as_bytes()),
            &["gpt-5.5"],
        );
        assert_eq!(
            again["llm-pi-ai"]["providers"]["ai-switch-claude"]["models"][0]["id"],
            "claude-opus-5"
        );
        assert_eq!(
            again["llm-pi-ai"]["providers"]["ai-switch-codex"]["models"][0]["id"],
            "gpt-5.5"
        );
    }

    #[test]
    fn render_writes_the_advertised_context_window_per_model() {
        // The harness ships no catalog entry for a hand-declared route, so an
        // entry carrying only an id inherits `defaultContextWindow` (256K) for
        // every model — truncating a 1M pool and over-claiming a 128K one.
        let models = vec![
            ClientModel {
                id: "gpt-5.6-sol".to_string(),
                context_window: 1_000_000,
                max_output_tokens: 128_000,
            },
            ClientModel {
                id: "gpt-5.5".to_string(),
                context_window: 128_000,
                max_output_tokens: 128_000,
            },
        ];
        let bytes = codex_adapter()
            .render(
                Path::new("settings.yaml"),
                None,
                &RouteConfigInput {
                    client_models: models,
                    ..input(&[])
                },
            )
            .expect("render");
        let yaml: Value = serde_yaml::from_slice(&bytes).expect("valid YAML");
        let entries = yaml["llm-pi-ai"]["providers"]["ai-switch-codex"]["models"]
            .as_sequence()
            .expect("models");

        assert_eq!(entries[0]["id"], "gpt-5.6-sol");
        assert_eq!(entries[0]["contextWindow"], 1_000_000);
        assert_eq!(entries[1]["id"], "gpt-5.5");
        assert_eq!(entries[1]["contextWindow"], 128_000);
        // An explicit `maxTokens` would become the model's per-request output
        // cap rather than metadata, so the pool's generic number stays out of it.
        assert!(entries[0].get("maxTokens").is_none());
    }

    #[test]
    fn adapter_identity_declares_deepseek_harness_as_restart_required_non_native() {
        for adapter in [codex_adapter(), claude_adapter()] {
            assert_eq!(adapter.client_key(), "deepseek_harness");
            assert!(!adapter.native());
            assert!(adapter.restart_required());
            assert!(adapter.requires_client_models());
        }
        assert_eq!(codex_adapter().target_key(), "deepseek_harness_codex");
        assert_eq!(claude_adapter().target_key(), "deepseek_harness_claude");
    }

    #[test]
    fn resolved_path_is_dsh_settings_yaml() {
        let home = Path::new("/home/user");
        assert_eq!(
            codex_adapter().resolve_path(home),
            home.join(".dsh").join("settings.yaml")
        );
    }

    #[test]
    fn render_writes_provider_with_managed_marker() {
        let yaml = render(codex_adapter().as_ref(), None, &["gpt-5.6-sol"]);
        let provider = &yaml["llm-pi-ai"]["providers"]["ai-switch-codex"];

        assert_eq!(provider["displayName"], "AI Switch (Codex)");
        // The route authenticates with a header, not a credential reference: a
        // dangling `apiKeyEnv` fails with MISSING_CREDENTIAL before any header is
        // tried, and nothing in this app supplies that reference.
        assert_eq!(
            provider["headers"]["Authorization"],
            "Bearer sk-ai-switch-test"
        );
        assert!(provider.get("apiKeyEnv").is_none());
        assert_eq!(provider["api"], "openai-completions");
        assert_eq!(provider["baseURL"], "http://127.0.0.1:19527/v1");
        assert_eq!(provider["aiSwitch"]["managed"], true);
        assert_eq!(provider["aiSwitch"]["platform"], "codex");
        assert_eq!(provider["models"][0]["id"], "gpt-5.6-sol");
    }

    #[test]
    fn render_preserves_other_providers_and_settings() {
        let existing = br#"
dsh-desktop:
  mode: compatibility
llm-pi-ai:
  providers:
    other-provider:
      displayName: Other
      api: openai-completions
      baseURL: https://example.com
"#;

        let yaml = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);

        assert_eq!(yaml["dsh-desktop"]["mode"], "compatibility");
        assert_eq!(
            yaml["llm-pi-ai"]["providers"]["other-provider"]["displayName"],
            "Other"
        );
        assert_eq!(
            yaml["llm-pi-ai"]["providers"]["ai-switch-codex"]["headers"]["Authorization"],
            "Bearer sk-ai-switch-test"
        );
    }

    #[test]
    fn a_legacy_dangling_credential_reference_is_replaced_by_the_header() {
        // Entries written before the header fix carry `apiKeyEnv:
        // AI_SWITCH_API_KEY`, a reference nothing in this app ever supplied. It
        // has to be removed, not merely joined by the header: a named-but-
        // unresolvable reference fails the request with MISSING_CREDENTIAL before
        // the header is consulted, so leaving it keeps the route broken.
        let existing = br#"
llm-pi-ai:
  providers:
    ai-switch-codex:
      displayName: AI Switch (Codex)
      apiKeyEnv: AI_SWITCH_API_KEY
      api: openai-completions
      baseURL: http://127.0.0.1:19527/v1
      headers:
        X-User-Added: keep-me
      aiSwitch:
        managed: true
        platform: codex
"#;

        let yaml = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let provider = &yaml["llm-pi-ai"]["providers"]["ai-switch-codex"];

        assert!(provider.get("apiKeyEnv").is_none());
        assert_eq!(
            provider["headers"]["Authorization"],
            "Bearer sk-ai-switch-test"
        );
        // A header the user added themselves is not collateral damage.
        assert_eq!(provider["headers"]["X-User-Added"], "keep-me");
    }

    #[test]
    fn adoption_claims_managed_entry_for_matching_platform() {
        let existing = br#"
llm-pi-ai:
  providers:
    renamed-entry:
      displayName: My Custom Name
      apiKeyEnv: AI_SWITCH_API_KEY
      api: openai-completions
      baseURL: http://127.0.0.1:1/v1
      aiSwitch:
        managed: true
        platform: codex
"#;

        let yaml = render(codex_adapter().as_ref(), Some(existing), &["gpt-5.6-sol"]);
        let provider = &yaml["llm-pi-ai"]["providers"]["renamed-entry"];

        assert!(yaml["llm-pi-ai"]["providers"]
            .get("ai-switch-codex")
            .is_none());
        assert_eq!(provider["displayName"], "My Custom Name");
        assert_eq!(provider["baseURL"], "http://127.0.0.1:19527/v1");
    }

    #[test]
    fn inspection_filters_by_platform() {
        let path = Path::new("settings.yaml");
        assert_eq!(codex_adapter().inspect(path, None).file_status, "missing");

        let claude_only = br#"
llm-pi-ai:
  providers:
    ai-switch-claude:
      apiKeyEnv: AI_SWITCH_API_KEY
      baseURL: http://127.0.0.1:19527/v1
      aiSwitch:
        managed: true
        platform: claude
"#;

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
    }
}
