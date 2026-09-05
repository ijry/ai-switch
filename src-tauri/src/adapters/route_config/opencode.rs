//! OpenCode native configuration writer.
//!
//! OpenCode reads `~/.config/opencode/opencode.json` and builds its provider
//! catalog from [models.dev] plus the overlay in that file's `provider` map. A
//! custom OpenAI-compatible provider needs four things — the AI SDK package,
//! the base URL, the credential, and an explicit model list, because a provider
//! that models.dev has never heard of contributes no models of its own.
//!
//! [models.dev]: https://models.dev

use super::{
    base_url_with_v1, existing_text, generated_invalid, invalid_existing_config, ClientModel,
    RouteConfigInput, TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Record key of the provider entry we own. Also the managed marker: OpenCode
/// keys providers by this id and the model ref is `<provider>/<model>`, so a
/// stable id is what lets a second write update the entry instead of adding a
/// twin the `/models` picker would show twice.
const PROVIDER_ID: &str = "ai-switch";

/// `@ai-sdk/openai-compatible` is the Chat Completions package. It is the one
/// wire shape the proxy bridges for every upstream dialect regardless of
/// platform, so it works whether the pool holds OpenAI, Responses, Anthropic or
/// Gemini accounts — `@ai-sdk/openai` would pin the client to `/responses`.
const NPM_PACKAGE: &str = "@ai-sdk/openai-compatible";

const SCHEMA_URL: &str = "https://opencode.ai/config.json";

pub(super) struct OpenCodeAdapter;

impl OpenCodeAdapter {
    fn model_entries(models: &[ClientModel]) -> Map<String, Value> {
        models
            .iter()
            .map(|model| {
                (
                    model.id.clone(),
                    // `limit` is how OpenCode knows how much context is left.
                    // Standard providers read it from models.dev; a custom one
                    // has to carry it.
                    json!({
                        "limit": {
                            "context": model.context_window,
                            "output": model.max_output_tokens,
                        },
                    }),
                )
            })
            .collect()
    }
}

impl TargetAdapter for OpenCodeAdapter {
    fn target_key(&self) -> &'static str {
        "opencode"
    }

    fn client_key(&self) -> &'static str {
        "opencode"
    }

    fn client_display_name(&self) -> &'static str {
        "OpenCode"
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        false
    }

    fn requires_client_models(&self) -> bool {
        true
    }

    fn platform(&self) -> PlatformId {
        PlatformId::OpenCode
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(".config").join("opencode").join("opencode.json")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let mut config = parse_existing(path, existing)?;
        let root = config
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "root value must be an object"))?;

        // Only on a file we are creating: an existing file may pin an older
        // schema on purpose, and rewriting that is not ours to do.
        root.entry("$schema".to_string())
            .or_insert_with(|| Value::String(SCHEMA_URL.to_string()));

        let providers = root
            .entry("provider".to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| invalid_existing_config(path, "JSON", "provider must be an object"))?;

        let existing_entry = providers.get(PROVIDER_ID).cloned();
        let entry = providers
            .entry(PROVIDER_ID.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                invalid_existing_config(path, "JSON", "provider entry must be an object")
            })?;

        entry.insert("npm".to_string(), Value::String(NPM_PACKAGE.to_string()));
        // A name the user edited is theirs; only fill one in when it is missing.
        let name = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("AI Switch")
            .to_string();
        entry.insert("name".to_string(), Value::String(name));

        let mut options = existing_entry
            .as_ref()
            .and_then(|entry| entry.get("options"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        options.insert(
            "baseURL".to_string(),
            Value::String(base_url_with_v1(&input.base_url)),
        );
        // Inline rather than through `/connect`: OpenCode stores credentials in
        // its own `auth.json` keyed by provider id, and we cannot mint an entry
        // there without owning that file too.
        options.insert(
            "apiKey".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );
        entry.insert("options".to_string(), Value::Object(options));
        entry.insert(
            "models".to_string(),
            Value::Object(Self::model_entries(&input.client_models)),
        );

        // Point the CLI at the pool. Without this the provider is merely
        // available and the user still has to pick it in `/models`, so a write
        // that reported success would change nothing they can see.
        if let Some(first) = input.client_models.first() {
            root.insert(
                "model".to_string(),
                Value::String(format!("{PROVIDER_ID}/{}", first.id)),
            );
        }

        let rendered =
            serde_json::to_vec_pretty(&config).map_err(|_| generated_invalid(path, "JSON"))?;
        if !serde_json::from_slice::<Value>(&rendered)
            .map_err(|_| generated_invalid(path, "JSON"))?
            .is_object()
        {
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
                .pointer(&format!("/provider/{PROVIDER_ID}"))
                .is_some_and(Value::is_object),
        )
    }
}

fn parse_existing(path: &Path, existing: Option<&[u8]>) -> Result<Value, AppError> {
    let Some(bytes) = existing else {
        return Ok(Value::Object(Map::new()));
    };
    let content = existing_text(path, "JSON", bytes)?;
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(content)
        .map_err(|_| invalid_existing_config(path, "JSON", "syntax is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{ClaudeEnvPlan, TargetAdapterRegistry};

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const KEY: &str = "sk-ai-switch-opencode";

    fn input(models: &[&str]) -> RouteConfigInput {
        RouteConfigInput {
            base_url: BASE_URL.to_string(),
            route_proxy_key: KEY.to_string(),
            route_proxy_key_aliases: Vec::new(),
            claude_env: ClaudeEnvPlan::default(),
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

    fn adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("opencode", PlatformId::OpenCode)
            .expect("opencode adapter")
    }

    fn render(existing: Option<&[u8]>, models: &[&str]) -> Value {
        let bytes = adapter()
            .render(Path::new("opencode.json"), existing, &input(models))
            .expect("render");
        serde_json::from_slice(&bytes).expect("valid JSON")
    }
    // PLACEHOLDER_OPENCODE_TESTS

    #[test]
    fn adapter_identity_declares_opencode_as_its_platforms_own_cli() {
        let adapter = adapter();
        assert_eq!(adapter.target_key(), "opencode");
        assert!(adapter.native(), "OpenCode is the OpenCode platform's CLI");
        // A CLI re-reads config on the next invocation.
        assert!(!adapter.restart_required());
        // models.dev knows nothing about this provider, so the write carries the
        // list or `/models` shows an empty provider.
        assert!(adapter.requires_client_models());
        assert_eq!(
            adapter.resolve_path(Path::new("/home/user")),
            Path::new("/home/user/.config/opencode/opencode.json")
        );
    }

    #[test]
    fn managed_entry_carries_the_chat_completions_package_credentials_and_limits() {
        let json = render(None, &["glm-5.3"]);
        let entry = &json["provider"]["ai-switch"];

        assert_eq!(entry["npm"], NPM_PACKAGE);
        // `@ai-sdk/openai-compatible` posts to `{baseURL}/chat/completions`, so
        // the `/v1` has to be in baseURL.
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");
        assert_eq!(entry["options"]["apiKey"], KEY);
        assert_eq!(
            entry["models"]["glm-5.3"]["limit"],
            json!({ "context": 200000, "output": 128000 })
        );
        // Without this the provider is merely available and the user still has
        // to pick it by hand.
        assert_eq!(json["model"], "ai-switch/glm-5.3");
        assert_eq!(json["$schema"], SCHEMA_URL);
    }

    #[test]
    fn render_preserves_other_providers_and_unmanaged_top_level_keys() {
        let existing = br#"{
  "$schema": "https://example.invalid/pinned.json",
  "theme": "gruvbox",
  "model": "anthropic/claude-opus-5",
  "provider": {
    "myprovider": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "https://api.example.com/v1", "apiKey": "sk-theirs" }
    }
  },
  "mcp": { "filesystem": { "type": "local", "command": ["npx"] } }
}"#;

        let json = render(Some(existing), &["glm-5.3"]);

        assert_eq!(json["theme"], "gruvbox");
        assert_eq!(
            json["provider"]["myprovider"]["options"]["apiKey"],
            "sk-theirs"
        );
        assert_eq!(json["mcp"]["filesystem"]["command"][0], "npx");
        // A pinned schema is the user's choice.
        assert_eq!(json["$schema"], "https://example.invalid/pinned.json");
        // The default model is what "接入算力池" means, so it does move.
        assert_eq!(json["model"], "ai-switch/glm-5.3");
    }

    #[test]
    fn a_second_write_updates_the_entry_in_place_and_keeps_a_renamed_label() {
        let existing = br#"{
  "provider": {
    "ai-switch": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "My Pool",
      "options": { "baseURL": "http://127.0.0.1:1/v1", "apiKey": "sk-stale", "headers": {"X-Trace": "1"} },
      "models": { "gone": {} }
    }
  }
}"#;

        let json = render(Some(existing), &["glm-5.3"]);
        let entry = &json["provider"]["ai-switch"];

        assert_eq!(json["provider"].as_object().unwrap().len(), 1);
        assert_eq!(entry["name"], "My Pool");
        assert_eq!(entry["options"]["apiKey"], KEY);
        assert_eq!(entry["options"]["baseURL"], "http://127.0.0.1:19527/v1");
        // Unmanaged option keys survive; the model list is the pool's and is
        // replaced wholesale so a dropped account's model disappears.
        assert_eq!(entry["options"]["headers"]["X-Trace"], "1");
        assert!(entry["models"].get("gone").is_none());
    }

    #[test]
    fn inspection_reports_missing_unmanaged_managed_and_invalid() {
        let adapter = adapter();
        let path = Path::new("opencode.json");

        assert_eq!(adapter.inspect(path, None).file_status, "missing");
        assert_eq!(
            adapter
                .inspect(path, Some(br#"{"theme":"gruvbox"}"#))
                .file_status,
            "unmanaged"
        );

        let managed = adapter
            .render(path, None, &input(&["glm-5.3"]))
            .expect("render");
        let inspection = adapter.inspect(path, Some(&managed));
        assert_eq!(inspection.file_status, "managed");
        assert!(inspection.managed);

        // JSON5 (comments, unquoted keys) and a non-object root both land here:
        // rewriting a file we cannot read would drop every provider in it.
        let invalid = adapter.inspect(path, Some(b"{ /* jsonc */ }"));
        assert_eq!(invalid.file_status, "invalid");
        assert_eq!(
            invalid.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
        assert_eq!(adapter.inspect(path, Some(b"[]")).file_status, "invalid");
    }

    #[test]
    fn corrupt_config_is_refused_rather_than_overwritten() {
        let adapter = adapter();
        let path = Path::new("opencode.json");
        for corrupt in [b"{not json".as_slice(), b"[]".as_slice()] {
            let error = adapter
                .render(path, Some(corrupt), &input(&["glm-5.3"]))
                .expect_err("must refuse");
            assert!(matches!(
                error,
                AppError::Validation {
                    code: "validation.route_config_existing_invalid",
                    ..
                }
            ));
        }

        // A blank file is not corrupt, just empty.
        assert_eq!(
            render(Some(b"   "), &["glm-5.3"])["provider"]["ai-switch"]["npm"],
            NPM_PACKAGE
        );
    }
}
