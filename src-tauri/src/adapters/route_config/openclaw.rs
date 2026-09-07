//! OpenClaw native configuration writer.
//!
//! OpenClaw reads `~/.openclaw/openclaw.json`. Custom and proxy endpoints live
//! under `models.providers.<id>`, and the agent picks one through the model ref
//! `agents.defaults.model.primary`, spelled `<providerId>/<modelId>`.
//!
//! The file is documented as JSON5. We parse it as strict JSON and refuse
//! anything else rather than round-tripping comments we cannot preserve — a
//! rewrite that dropped the user's `models.providers` would take every provider
//! they have with it.

use super::{
    base_url_with_v1, existing_text, generated_invalid, invalid_existing_config, ClientModel,
    RouteConfigInput, TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Record key of the provider entry we own, and the first half of every model
/// ref we write. Stable so a second write updates the entry in place.
const PROVIDER_ID: &str = "ai-switch";

/// OpenClaw's Chat Completions protocol. The one wire shape the proxy bridges
/// for every upstream dialect regardless of platform, so the same entry serves
/// an OpenAI, Responses, Anthropic or Gemini pool.
const API_PROTOCOL: &str = "openai-completions";

pub(super) struct OpenClawAdapter;

impl OpenClawAdapter {
    fn model_entries(models: &[ClientModel]) -> Vec<Value> {
        models
            .iter()
            .map(|model| {
                // `contextWindow` is the native window and `maxTokens` the output
                // ceiling. Both are optional, but an omitted window makes
                // OpenClaw fall back to a flat 200K for context budgeting.
                json!({
                    "id": model.id,
                    "contextWindow": model.context_window,
                    "maxTokens": model.max_output_tokens,
                })
            })
            .collect()
    }
}

impl TargetAdapter for OpenClawAdapter {
    fn target_key(&self) -> &'static str {
        "openclaw"
    }

    fn client_key(&self) -> &'static str {
        "openclaw"
    }

    fn client_display_name(&self) -> &'static str {
        "OpenClaw"
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        // The gateway watches this file and hot-reloads it, but a session
        // already running keeps the model it started with.
        false
    }

    fn requires_client_models(&self) -> bool {
        true
    }

    fn platform(&self) -> PlatformId {
        PlatformId::OpenClaw
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        home.join(".openclaw").join("openclaw.json")
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

        let models = object_at(root, "models")
            .ok_or_else(|| invalid_existing_config(path, "JSON", "models must be an object"))?;
        // Only when absent: `replace` is a deliberate choice we must not undo,
        // and leaving it unset would be one more thing the user has to know.
        models
            .entry("mode".to_string())
            .or_insert_with(|| Value::String("merge".to_string()));

        let providers = object_at(models, "providers").ok_or_else(|| {
            invalid_existing_config(path, "JSON", "models.providers must be an object")
        })?;
        let entry = object_at(providers, PROVIDER_ID).ok_or_else(|| {
            invalid_existing_config(path, "JSON", "provider entry must be an object")
        })?;
        entry.insert(
            "baseUrl".to_string(),
            Value::String(base_url_with_v1(&input.base_url)),
        );
        entry.insert(
            "apiKey".to_string(),
            Value::String(input.route_proxy_key.clone()),
        );
        entry.insert("api".to_string(), Value::String(API_PROTOCOL.to_string()));
        entry.insert(
            "models".to_string(),
            Value::Array(Self::model_entries(&input.client_models)),
        );

        // Point the gateway at the pool. Adding a provider without selecting it
        // changes nothing the user can see.
        if let Some(first) = input.client_models.first() {
            let defaults = object_at(root, "agents")
                .and_then(|agents| object_at(agents, "defaults"))
                .and_then(|defaults| object_at(defaults, "model"))
                .ok_or_else(|| {
                    invalid_existing_config(path, "JSON", "agents.defaults.model must be an object")
                })?;
            defaults.insert(
                "primary".to_string(),
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
                .pointer(&format!("/models/providers/{PROVIDER_ID}"))
                .is_some_and(Value::is_object),
        )
    }
}

/// The object at `key`, created when absent. `None` when the key holds
/// something that is not an object, so the caller can refuse the whole write
/// instead of replacing the user's value.
fn object_at<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> Option<&'a mut Map<String, Value>> {
    parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
}

fn parse_existing(path: &Path, existing: Option<&[u8]>) -> Result<Value, AppError> {
    let Some(bytes) = existing else {
        return Ok(Value::Object(Map::new()));
    };
    let content = existing_text(path, "JSON", bytes)?;
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(content).map_err(|_| {
        invalid_existing_config(
            path,
            "JSON",
            "syntax is invalid; JSON5 comments and unquoted keys are not supported",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{ClaudeEnvPlan, TargetAdapterRegistry};

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const KEY: &str = "sk-ai-switch-openclaw";

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
                    reasoning_levels: Vec::new(),
                })
                .collect(),
        }
    }

    fn adapter() -> std::sync::Arc<dyn TargetAdapter> {
        TargetAdapterRegistry::new()
            .by_client_and_platform("openclaw", PlatformId::OpenClaw)
            .expect("openclaw adapter")
    }

    fn render(existing: Option<&[u8]>, models: &[&str]) -> Value {
        let bytes = adapter()
            .render(Path::new("openclaw.json"), existing, &input(models))
            .expect("render");
        serde_json::from_slice(&bytes).expect("valid JSON")
    }
    // PLACEHOLDER_OPENCLAW_TESTS

    #[test]
    fn adapter_identity_and_path() {
        let adapter = adapter();
        assert_eq!(adapter.target_key(), "openclaw");
        assert!(adapter.native());
        assert!(adapter.requires_client_models());
        assert_eq!(
            adapter.resolve_path(Path::new("/home/user")),
            Path::new("/home/user/.openclaw/openclaw.json")
        );
    }

    #[test]
    fn managed_provider_carries_the_completions_protocol_credentials_and_limits() {
        let json = render(None, &["glm-5.3", "kimi-k3"]);
        let entry = &json["models"]["providers"]["ai-switch"];

        assert_eq!(entry["api"], API_PROTOCOL);
        // `openai-completions` posts to `{baseUrl}/chat/completions`.
        assert_eq!(entry["baseUrl"], "http://127.0.0.1:19527/v1");
        assert_eq!(entry["apiKey"], KEY);
        assert_eq!(
            entry["models"],
            json!([
                { "id": "glm-5.3", "contextWindow": 200000, "maxTokens": 128000 },
                { "id": "kimi-k3", "contextWindow": 200000, "maxTokens": 128000 },
            ])
        );
        // `merge` keeps the bundled catalog alongside our overlay.
        assert_eq!(json["models"]["mode"], "merge");
        assert_eq!(
            json["agents"]["defaults"]["model"]["primary"],
            "ai-switch/glm-5.3"
        );
    }

    #[test]
    fn render_preserves_foreign_providers_an_explicit_mode_and_unrelated_sections() {
        let existing = br#"{
  "models": {
    "mode": "replace",
    "providers": {
      "lmstudio": { "baseUrl": "http://localhost:1234/v1", "api": "openai-completions" }
    }
  },
  "agents": { "defaults": { "workspace": "~/.openclaw/workspace", "model": { "primary": "anthropic/claude-opus-5", "fallbacks": ["openai/gpt-5.6"] } } },
  "mcp": { "servers": { "demo": { "type": "http", "url": "https://example.test/mcp" } } }
}"#;

        let json = render(Some(existing), &["glm-5.3"]);

        // An explicit `replace` is a deliberate choice.
        assert_eq!(json["models"]["mode"], "replace");
        assert_eq!(
            json["models"]["providers"]["lmstudio"]["baseUrl"],
            "http://localhost:1234/v1"
        );
        assert_eq!(
            json["agents"]["defaults"]["workspace"],
            "~/.openclaw/workspace"
        );
        assert_eq!(json["mcp"]["servers"]["demo"]["type"], "http");
        // The fallback chain is the user's; only the primary moves.
        assert_eq!(
            json["agents"]["defaults"]["model"]["fallbacks"][0],
            "openai/gpt-5.6"
        );
        assert_eq!(
            json["agents"]["defaults"]["model"]["primary"],
            "ai-switch/glm-5.3"
        );
    }

    #[test]
    fn inspection_and_refusal() {
        let adapter = adapter();
        let path = Path::new("openclaw.json");

        assert_eq!(adapter.inspect(path, None).file_status, "missing");
        assert_eq!(
            adapter
                .inspect(path, Some(br#"{"models":{"providers":{}}}"#))
                .file_status,
            "unmanaged"
        );
        let managed = adapter
            .render(path, None, &input(&["glm-5.3"]))
            .expect("render");
        assert_eq!(adapter.inspect(path, Some(&managed)).file_status, "managed");

        // JSON5 is what the docs advertise, so this is the shape we must refuse
        // loudly rather than overwrite.
        let json5 = b"{\n  // pool\n  models: { mode: 'merge' },\n}\n";
        assert_eq!(adapter.inspect(path, Some(json5)).file_status, "invalid");
        let error = adapter
            .render(path, Some(json5), &input(&["glm-5.3"]))
            .expect_err("must refuse");
        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.route_config_existing_invalid",
                ..
            }
        ));
    }

    #[test]
    fn a_non_object_at_a_key_we_write_is_refused_instead_of_replaced() {
        let adapter = adapter();
        let path = Path::new("openclaw.json");
        for existing in [
            br#"{"models": "merge"}"#.as_slice(),
            br#"{"models": {"providers": []}}"#.as_slice(),
            br#"{"agents": {"defaults": {"model": "anthropic/claude-opus-5"}}}"#.as_slice(),
        ] {
            let error = adapter
                .render(path, Some(existing), &input(&["glm-5.3"]))
                .expect_err("must refuse");
            assert_eq!(error.code(), "validation.route_config_existing_invalid");
        }
    }
}
