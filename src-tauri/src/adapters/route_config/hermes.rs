//! Hermes Agent native configuration writer.
//!
//! Hermes reads `~/.hermes/config.yaml`. Writable third-party endpoints live in
//! the `custom_providers:` sequence, keyed by `name`, and the agent selects one
//! through `model.provider` plus `model.default`.
//!
//! `api_key` is written inline on purpose. Hermes otherwise resolves a
//! provider's credential from `~/.hermes/.env` or from an env var derived from
//! the endpoint's **host** (`OPENROUTER_API_KEY` for openrouter.ai, and so on).
//! A loopback pool endpoint matches no host, so a hand-written entry carrying
//! only `base_url` leaves Hermes with nowhere to read the key from — which is
//! the "missing API key" every hand-configured pool ran into.
//!
//! Only the two sections we own are rewritten, textually. `config.yaml` ships
//! with extensive comments and Hermes' own docs tell people to hand-edit it, so
//! re-serializing the whole document would silently strip all of that.

use super::{
    base_url_with_v1, existing_text, generated_invalid, invalid_existing_config, ClientModel,
    RouteConfigInput, TargetAdapter, TargetInspection,
};
use crate::{error::AppError, models::platform::PlatformId};
use serde_yaml::{Mapping, Value};
use std::path::{Path, PathBuf};

/// `custom_providers[].name` of the entry we own, and the value we put in
/// `model.provider`.
const PROVIDER_NAME: &str = "ai-switch";

/// Hermes' Chat Completions protocol. The one wire shape the proxy bridges for
/// every upstream dialect regardless of platform, so the same entry serves an
/// OpenAI, Responses, Anthropic or Gemini pool. Always written explicitly:
/// auto-detection keys off the endpoint host and a loopback host tells it
/// nothing.
const API_MODE: &str = "chat_completions";

const PROVIDERS_SECTION: &str = "custom_providers";
const MODEL_SECTION: &str = "model";

pub(super) struct HermesAdapter;

impl HermesAdapter {
    /// Our `custom_providers` entry. `preserved` carries fields the user or a
    /// newer Hermes put on the entry (`key_env`, `request_timeout_seconds`, …)
    /// so a write does not strip them.
    fn provider_entry(input: &RouteConfigInput, preserved: Option<&Mapping>) -> Value {
        let mut entry = Mapping::new();
        entry.insert(string("name"), string(PROVIDER_NAME));
        entry.insert(
            string("base_url"),
            string(&base_url_with_v1(&input.base_url)),
        );
        entry.insert(string("api_key"), string(&input.route_proxy_key));
        entry.insert(string("api_mode"), string(API_MODE));
        // Singular `model`: what the runtime and the `/model` picker read off
        // the entry. Plural `models` only carries per-model metadata.
        if let Some(first) = input.client_models.first() {
            entry.insert(string("model"), string(&first.id));
        }
        entry.insert(string("models"), Self::model_entries(&input.client_models));

        if let Some(preserved) = preserved {
            for (key, value) in preserved {
                if !entry.contains_key(key) {
                    entry.insert(key.clone(), value.clone());
                }
            }
        }
        Value::Mapping(entry)
    }

    /// Per-model metadata. `context_length` only: Hermes'
    /// `_VALID_CUSTOM_PROVIDER_FIELDS` has no per-model `max_tokens`, and an
    /// unknown field there warns on every startup.
    fn model_entries(models: &[ClientModel]) -> Value {
        let mut map = Mapping::new();
        for model in models {
            let mut entry = Mapping::new();
            entry.insert(
                string("context_length"),
                Value::Number(model.context_window.into()),
            );
            map.insert(string(&model.id), Value::Mapping(entry));
        }
        Value::Mapping(map)
    }

    /// The `model:` section pointed at the pool, keeping every key we do not own.
    fn model_section(input: &RouteConfigInput, existing: Option<&Mapping>) -> Value {
        let mut section = existing.cloned().unwrap_or_default();
        section.insert(string("provider"), string(PROVIDER_NAME));
        // Written rather than cleared: whichever of the two Hermes reads, both
        // then agree. A leftover value from the previous provider would not.
        section.insert(
            string("base_url"),
            string(&base_url_with_v1(&input.base_url)),
        );
        section.insert(string("api_mode"), string(API_MODE));
        if let Some(first) = input.client_models.first() {
            section.insert(string("default"), string(&first.id));
        }
        Value::Mapping(section)
    }
}

impl TargetAdapter for HermesAdapter {
    fn target_key(&self) -> &'static str {
        "hermes"
    }

    fn client_key(&self) -> &'static str {
        "hermes"
    }

    fn client_display_name(&self) -> &'static str {
        "Hermes Agent"
    }

    fn native(&self) -> bool {
        true
    }

    fn restart_required(&self) -> bool {
        // The CLI reads config on the next invocation; a running gateway keeps
        // its sessions on the model they started with.
        false
    }

    fn requires_client_models(&self) -> bool {
        true
    }

    fn platform(&self) -> PlatformId {
        PlatformId::Hermes
    }

    fn resolve_path(&self, home: &Path) -> PathBuf {
        hermes_home(home, std::env::var("HERMES_HOME").ok().as_deref()).join("config.yaml")
    }

    fn render(
        &self,
        path: &Path,
        existing: Option<&[u8]>,
        input: &RouteConfigInput,
    ) -> Result<Vec<u8>, AppError> {
        let raw = match existing {
            Some(bytes) => existing_text(path, "YAML", bytes)?,
            None => "",
        };
        let root = parse_root(path, raw)?;

        let preserved_entry = providers_sequence(&root)
            .iter()
            .find(|entry| entry_name(entry) == Some(PROVIDER_NAME))
            .and_then(Value::as_mapping)
            .cloned();
        let mut providers = providers_sequence(&root);
        let entry = HermesAdapter::provider_entry(input, preserved_entry.as_ref());
        match providers
            .iter()
            .position(|entry| entry_name(entry) == Some(PROVIDER_NAME))
        {
            Some(index) => providers[index] = entry,
            None => providers.push(entry),
        }

        let model = HermesAdapter::model_section(
            input,
            root.get(MODEL_SECTION).and_then(Value::as_mapping),
        );

        let rendered = replace_section(raw, PROVIDERS_SECTION, &Value::Sequence(providers))?;
        let rendered = replace_section(&rendered, MODEL_SECTION, &model)?;

        // The section surgery is textual, so prove the result still parses and
        // still says what we meant before it can reach the user's file.
        let reparsed: Value =
            serde_yaml::from_str(&rendered).map_err(|_| generated_invalid(path, "YAML"))?;
        let says_ai_switch = reparsed
            .get(MODEL_SECTION)
            .and_then(|model| model.get("provider"))
            .and_then(Value::as_str)
            == Some(PROVIDER_NAME)
            && providers_sequence(&reparsed)
                .iter()
                .any(|entry| entry_name(entry) == Some(PROVIDER_NAME));
        if !says_ai_switch {
            return Err(generated_invalid(path, "YAML"));
        }
        Ok(rendered.into_bytes())
    }

    fn inspect(&self, path: &Path, existing: Option<&[u8]>) -> TargetInspection {
        let Some(bytes) = existing else {
            return TargetInspection::missing();
        };
        let Ok(raw) = std::str::from_utf8(bytes) else {
            return TargetInspection::invalid();
        };
        let Ok(root) = parse_root(path, raw) else {
            return TargetInspection::invalid();
        };
        TargetInspection::valid(
            providers_sequence(&root)
                .iter()
                .any(|entry| entry_name(entry) == Some(PROVIDER_NAME)),
        )
    }
}

/// Hermes' state directory. `HERMES_HOME` wins when set, matching the MCP and
/// skills path resolvers.
///
/// Only an absolute override is honored. Hermes itself takes the value verbatim,
/// but the config-write machinery snapshots this path and later re-resolves it to
/// verify nothing was tampered with, and a relative path resolves against
/// whatever the working directory happens to be at each of those moments.
fn hermes_home(home: &Path, override_value: Option<&str>) -> PathBuf {
    match override_value.map(str::trim) {
        Some(value) if Path::new(value).is_absolute() => PathBuf::from(value),
        _ => home.join(".hermes"),
    }
}

fn string(value: &str) -> Value {
    Value::String(value.to_string())
}

fn entry_name(entry: &Value) -> Option<&str> {
    entry.get("name").and_then(Value::as_str).map(str::trim)
}

fn providers_sequence(root: &Value) -> Vec<Value> {
    root.get(PROVIDERS_SECTION)
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default()
}

fn parse_root(path: &Path, raw: &str) -> Result<Value, AppError> {
    if raw.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    let root: Value = serde_yaml::from_str(raw)
        .map_err(|error| invalid_existing_config(path, "YAML", &error.to_string()))?;
    if root.is_null() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    if !root.is_mapping() {
        return Err(invalid_existing_config(
            path,
            "YAML",
            "root value must be a mapping",
        ));
    }
    Ok(root)
}

/// A line that opens a top-level mapping key: column 0, not a comment, not a
/// sequence item, and `key:` followed by end-of-line or whitespace.
fn is_top_level_key_line(line: &str) -> bool {
    let Some(first) = line.as_bytes().first() else {
        return false;
    };
    if matches!(first, b' ' | b'\t' | b'#' | b'-') {
        return false;
    }
    match line.find(':') {
        Some(colon) => {
            let rest = &line[colon + 1..];
            rest.is_empty() || rest.starts_with([' ', '\t', '\r'])
        }
        None => false,
    }
}

/// Byte range of one top-level section: from its key line to the next top-level
/// key line, or end of text.
fn section_range(raw: &str, key: &str) -> Option<(usize, usize)> {
    let target = format!("{key}:");
    let mut start = None;
    let mut offset = 0;

    for line in raw.split('\n') {
        if start.is_none() {
            let opens_target = is_top_level_key_line(line)
                && line.starts_with(&target)
                && line[target.len()..]
                    .chars()
                    .next()
                    .is_none_or(|next| next.is_whitespace());
            if opens_target {
                start = Some(offset);
            }
        } else if is_top_level_key_line(line) {
            return Some((start.unwrap(), offset));
        }
        offset += line.len() + 1;
    }

    start.map(|start| (start, raw.len()))
}

/// Replaces one top-level section, appending it when absent. Everything outside
/// the section — comments included — is carried through byte for byte.
fn replace_section(raw: &str, key: &str, value: &Value) -> Result<String, AppError> {
    let mut section = Mapping::new();
    section.insert(string(key), value.clone());
    let mut serialized =
        serde_yaml::to_string(&Value::Mapping(section)).map_err(|error| AppError::Validation {
            code: "config.generated_invalid",
            message: "Could not serialize the Hermes configuration section".to_string(),
            details: Some(error.to_string()),
            recoverable: false,
        })?;
    if !serialized.ends_with('\n') {
        serialized.push('\n');
    }
    // Keep the file's own line endings so a rewrite does not leave it mixed.
    if raw.contains("\r\n") {
        serialized = serialized.replace('\n', "\r\n");
    }

    let Some((start, end)) = section_range(raw, key) else {
        let mut result = raw.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&serialized);
        return Ok(result);
    };
    Ok(format!("{}{serialized}{}", &raw[..start], &raw[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::route_config::{ClaudeEnvPlan, TargetAdapterRegistry};

    const BASE_URL: &str = "http://127.0.0.1:19527";
    const KEY: &str = "sk-ai-switch-hermes";

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
            .by_client_and_platform("hermes", PlatformId::Hermes)
            .expect("hermes adapter")
    }

    fn render_text(existing: Option<&[u8]>, models: &[&str]) -> String {
        let bytes = adapter()
            .render(Path::new("config.yaml"), existing, &input(models))
            .expect("render");
        String::from_utf8(bytes).expect("valid UTF-8")
    }

    fn render(existing: Option<&[u8]>, models: &[&str]) -> Value {
        serde_yaml::from_str(&render_text(existing, models)).expect("valid YAML")
    }

    fn provider(root: &Value) -> &Value {
        providers_sequence(root)
            .iter()
            .position(|entry| entry_name(entry) == Some(PROVIDER_NAME))
            .map(|index| &root[PROVIDERS_SECTION][index])
            .expect("managed provider entry")
    }
    // PLACEHOLDER_HERMES_TESTS

    #[test]
    fn adapter_identity_and_path() {
        let adapter = adapter();
        assert_eq!(adapter.target_key(), "hermes");
        assert!(adapter.native());
        assert!(adapter.requires_client_models());
        assert_eq!(
            adapter.resolve_path(Path::new("/home/user")),
            Path::new("/home/user/.hermes/config.yaml")
        );
    }

    #[test]
    fn hermes_home_honors_an_absolute_override_and_ignores_a_relative_one() {
        let home = Path::new("/home/user");
        let absolute = std::env::temp_dir().join("hermes-state");
        assert_eq!(
            hermes_home(home, Some(&absolute.display().to_string())),
            absolute
        );
        // A relative override would resolve against whatever the working
        // directory happens to be, and the snapshot path check re-resolves it
        // later — so it is refused in favor of the default.
        assert_eq!(hermes_home(home, Some("./hermes")), home.join(".hermes"));
        assert_eq!(hermes_home(home, Some("  ")), home.join(".hermes"));
        assert_eq!(hermes_home(home, None), home.join(".hermes"));
    }

    #[test]
    fn the_entry_carries_the_api_key_inline_because_hermes_has_no_host_to_derive_it_from() {
        let root = render(None, &["glm-5.3"]);
        let entry = provider(&root);

        // The whole point: without this Hermes looks for a key in .env or in an
        // env var named after the endpoint host, finds neither for 127.0.0.1,
        // and reports a missing API key.
        assert_eq!(entry["api_key"], Value::String(KEY.to_string()));
        assert_eq!(
            entry["base_url"],
            Value::String("http://127.0.0.1:19527/v1".to_string())
        );
        assert_eq!(entry["api_mode"], Value::String(API_MODE.to_string()));
        // Singular: what the runtime and the `/model` picker read.
        assert_eq!(entry["model"], Value::String("glm-5.3".to_string()));
        assert_eq!(entry["models"]["glm-5.3"]["context_length"], 200_000);
        // Per-model max_tokens is not a valid Hermes field and warns on startup.
        assert!(entry["models"]["glm-5.3"].get("max_tokens").is_none());

        assert_eq!(
            root["model"]["provider"],
            Value::String(PROVIDER_NAME.to_string())
        );
        assert_eq!(
            root["model"]["default"],
            Value::String("glm-5.3".to_string())
        );
        assert_eq!(
            root["model"]["base_url"],
            Value::String("http://127.0.0.1:19527/v1".to_string())
        );
    }

    #[test]
    fn only_the_two_sections_we_own_are_rewritten_comments_and_all() {
        let existing = br#"# Hermes Agent CLI Configuration
database:
  journal_mode: "wal"  # keep WAL

model:
  # hand-picked
  default: "anthropic/claude-opus-4.6"
  provider: "openrouter"
  base_url: "https://openrouter.ai/api/v1"
  max_tokens: 8192

custom_providers:
  - name: openrouter
    base_url: https://openrouter.ai/api/v1
    api_key: sk-or-theirs

mcp_servers:
  filesystem:
    command: npx
"#;

        let text = render_text(Some(existing), &["glm-5.3"]);
        let root: Value = serde_yaml::from_str(&text).expect("valid YAML");

        // Untouched sections keep their comments byte for byte.
        assert!(text.contains("# Hermes Agent CLI Configuration"));
        assert!(text.contains("journal_mode: \"wal\"  # keep WAL"));
        assert!(text.contains("mcp_servers:"));
        assert!(text.contains("command: npx"));
        // Their own provider survives; ours is appended.
        assert_eq!(providers_sequence(&root).len(), 2);
        assert_eq!(
            root["custom_providers"][0]["api_key"],
            Value::String("sk-or-theirs".to_string())
        );
        // Keys we do not own inside `model:` are kept.
        assert_eq!(root["model"]["max_tokens"], 8192);
        assert_eq!(
            root["model"]["provider"],
            Value::String(PROVIDER_NAME.to_string())
        );
    }

    #[test]
    fn a_second_write_updates_our_entry_in_place_and_keeps_its_unknown_fields() {
        let first = render_text(None, &["glm-5.3"]);
        let with_extra = first.replace(
            "  api_mode: chat_completions\n",
            "  api_mode: chat_completions\n  request_timeout_seconds: 600\n",
        );
        assert!(with_extra.contains("request_timeout_seconds"));

        let root: Value =
            serde_yaml::from_str(&render_text(Some(with_extra.as_bytes()), &["kimi-k3"]))
                .expect("valid YAML");

        assert_eq!(providers_sequence(&root).len(), 1);
        let entry = provider(&root);
        // A field a newer Hermes (or the user) added is not ours to drop.
        assert_eq!(entry["request_timeout_seconds"], 600);
        // The model list belongs to the pool and is replaced wholesale.
        assert!(entry["models"].get("glm-5.3").is_none());
        assert_eq!(entry["model"], Value::String("kimi-k3".to_string()));
    }

    #[test]
    fn rendering_is_stable_so_a_written_config_never_reads_as_stale() {
        let once = render_text(None, &["glm-5.3"]);
        let twice = render_text(Some(once.as_bytes()), &["glm-5.3"]);
        assert_eq!(once, twice);
    }

    #[test]
    fn inspection_reports_missing_unmanaged_managed_and_invalid() {
        let adapter = adapter();
        let path = Path::new("config.yaml");

        assert_eq!(adapter.inspect(path, None).file_status, "missing");
        assert_eq!(
            adapter
                .inspect(path, Some(b"custom_providers:\n  - name: openrouter\n"))
                .file_status,
            "unmanaged"
        );

        let managed = render_text(None, &["glm-5.3"]);
        let inspection = adapter.inspect(path, Some(managed.as_bytes()));
        assert_eq!(inspection.file_status, "managed");
        assert!(inspection.managed);

        let invalid = adapter.inspect(path, Some(b"model:\n\tdefault: x\n"));
        assert_eq!(invalid.file_status, "invalid");
        assert_eq!(
            invalid.error_code.as_deref(),
            Some("validation.route_config_existing_invalid")
        );
    }

    #[test]
    fn unreadable_config_is_refused_rather_than_overwritten() {
        let adapter = adapter();
        let path = Path::new("config.yaml");
        // Tabs are illegal YAML indentation, and a sequence root has nowhere to
        // put `model:`. Overwriting either would cost the user every provider.
        for corrupt in [
            b"model:\n\tdefault: x\n".as_slice(),
            b"- one\n- two\n".as_slice(),
        ] {
            let error = adapter
                .render(path, Some(corrupt), &input(&["glm-5.3"]))
                .expect_err("must refuse");
            assert_eq!(error.code(), "validation.route_config_existing_invalid");
        }

        // A blank file is not corrupt.
        assert_eq!(
            render(Some(b"\n  \n"), &["glm-5.3"])["model"]["provider"],
            Value::String(PROVIDER_NAME.to_string())
        );
    }

    #[test]
    fn crlf_files_keep_crlf() {
        let existing = b"model:\r\n  default: old\r\n\r\ndatabase:\r\n  journal_mode: wal\r\n";
        let text = render_text(Some(existing), &["glm-5.3"]);
        assert!(text.contains("provider: ai-switch\r\n"), "{text}");
        assert!(!text.contains("provider: ai-switch\n\n"), "{text}");
        assert!(text.contains("journal_mode: wal\r\n"));
    }
}
