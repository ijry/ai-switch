use crate::models::route_credential::{is_fallback_mapping, ModelMapping};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelCapability {
    pub(crate) mappings: Vec<ModelMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdvertisedModel {
    pub(crate) id: String,
    description: String,
    /// The model this alias is rewritten to. Needed because the default context
    /// window is a property of the *upstream* model, not of the alias the client
    /// asks for. A baseline (wildcard) entry is not rewritten, so its id doubles
    /// as its upstream name.
    pub(crate) upstream_model: String,
    /// Per-alias overrides carried over from the mapping that produced this
    /// entry. `None` keeps the platform default, so a baseline (wildcard) model
    /// and a pre-existing mapping behave exactly as they did before.
    pub(crate) context_window: Option<u32>,
    pub(crate) reasoning_levels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CodexReasoningProfile {
    pub(crate) levels: &'static [&'static str],
    pub(crate) default_level: &'static str,
}

const SOL_REASONING_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
const TERRA_REASONING_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
const LUNA_REASONING_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const GPT_55_REASONING_LEVELS: &[&str] = &["low", "medium", "high", "xhigh"];
const DEFAULT_REASONING_LEVELS: &[&str] = &["low", "medium", "high"];

/// Context window written into the Codex catalog when a mapping declares none
/// and its upstream model is not one of the known 1M families.
pub(crate) const CODEX_DEFAULT_CONTEXT_WINDOW: u32 = 256_000;
/// Decimal 1M on purpose. `1_048_576` is how the CPA transfer format marks a
/// Claude 1M tier, and reusing it would make a Codex row come back from a round
/// trip looking like one.
pub(crate) const CODEX_ONE_M_CONTEXT_WINDOW: u32 = 1_000_000;

/// Upstream model families that really serve 1M context, matched on the start of
/// the mapped-to name so every dated or sized variant is covered
/// (`deepseek-v4-flash-0731`, `glm-5.3-air`, …).
const CODEX_ONE_M_UPSTREAM_PREFIXES: &[&str] =
    &["deepseek-v4", "glm-5.2", "glm-5.3", "qwen-3.8", "kimi-k3"];

/// The window to advertise for an alias whose mapping declares none.
///
/// Relays publish these families under a vendor path as often as bare
/// (`z-ai/glm-5.3`), so the last path segment is tried too — otherwise the same
/// model would silently fall back to the generic default on half the relays.
pub(crate) fn codex_default_context_window(upstream_model: &str) -> u32 {
    let name = upstream_model.trim().to_ascii_lowercase();
    let bare = name.rsplit('/').next().unwrap_or(name.as_str());
    if CODEX_ONE_M_UPSTREAM_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix) || bare.starts_with(prefix))
    {
        CODEX_ONE_M_CONTEXT_WINDOW
    } else {
        CODEX_DEFAULT_CONTEXT_WINDOW
    }
}

/// What Codex clients are told, preferring the user's own declaration. Zero is
/// treated as "not declared": it is not a window anything could serve.
pub(crate) fn codex_effective_context_window(declared: Option<u32>, upstream_model: &str) -> u32 {
    declared
        .filter(|window| *window > 0)
        .unwrap_or_else(|| codex_default_context_window(upstream_model))
}

pub(crate) fn codex_reasoning_profile(model: &str) -> CodexReasoningProfile {
    match model.trim().to_ascii_lowercase().as_str() {
        "gpt-5.6-sol" => CodexReasoningProfile {
            levels: SOL_REASONING_LEVELS,
            default_level: "low",
        },
        "gpt-5.6-terra" => CodexReasoningProfile {
            levels: TERRA_REASONING_LEVELS,
            default_level: "medium",
        },
        "gpt-5.6-luna" => CodexReasoningProfile {
            levels: LUNA_REASONING_LEVELS,
            default_level: "medium",
        },
        "gpt-5.5" => CodexReasoningProfile {
            levels: GPT_55_REASONING_LEVELS,
            default_level: "medium",
        },
        _ => CodexReasoningProfile {
            levels: DEFAULT_REASONING_LEVELS,
            default_level: "medium",
        },
    }
}

/// The efforts advertised for one alias: the mapping's own list when it declares
/// one, else the baseline profile for that model id. Entries are trimmed,
/// lowercased and deduped so a hand-edited config cannot put `["High","high"]`
/// in front of the Codex CLI; an unrecognised effort is kept and simply gets the
/// generic description, which is what lets a new upstream tier work before this
/// table learns about it.
pub(crate) fn codex_reasoning_levels(model: &str, overrides: Option<&[String]>) -> Vec<String> {
    let declared = overrides
        .map(|levels| {
            let mut seen = HashSet::new();
            levels
                .iter()
                .map(|level| level.trim().to_ascii_lowercase())
                .filter(|level| !level.is_empty() && seen.insert(level.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !declared.is_empty() {
        return declared;
    }
    codex_reasoning_profile(model)
        .levels
        .iter()
        .map(|level| (*level).to_string())
        .collect()
}

pub(crate) fn codex_reasoning_metadata(
    model: &str,
    overrides: Option<&[String]>,
) -> (Vec<Value>, String) {
    let profile = codex_reasoning_profile(model);
    let levels = codex_reasoning_levels(model, overrides);
    // A custom list that drops the profile default would otherwise leave Codex
    // preselecting an effort it was just told is unavailable.
    let default_level = levels
        .iter()
        .find(|level| level.as_str() == profile.default_level)
        .or_else(|| levels.first())
        .map(String::as_str)
        .unwrap_or(profile.default_level)
        .to_string();
    let levels = levels
        .into_iter()
        .map(|effort| {
            json!({
                "effort": effort,
                "description": codex_reasoning_description(&effort),
            })
        })
        .collect();
    (levels, default_level)
}

pub(crate) fn codex_model_catalog_payload(capabilities: &[ModelCapability]) -> Value {
    let models = advertised_model_catalog_entries("codex", capabilities)
        .into_iter()
        .enumerate()
        .map(|(index, model)| {
            let (supported_reasoning_levels, default_reasoning_level) =
                codex_reasoning_metadata(&model.id, model.reasoning_levels.as_deref());
            let context_window =
                codex_effective_context_window(model.context_window, &model.upstream_model);
            json!({
                "additional_speed_tiers": [],
                "availability_nux": null,
                "base_instructions": "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals.",
                "context_window": context_window,
                "default_reasoning_level": default_reasoning_level,
                "default_reasoning_summary": "none",
                "description": model.description,
                "display_name": model.id,
                "effective_context_window_percent": 95,
                "experimental_supported_tools": [],
                "input_modalities": ["text", "image"],
                "max_context_window": context_window,
                "priority": index as i32 + 1,
                "service_tiers": [],
                "shell_type": "shell_command",
                "slug": model.id,
                "support_verbosity": false,
                "supported_in_api": true,
                "supported_reasoning_levels": supported_reasoning_levels,
                "supports_image_detail_original": false,
                "supports_parallel_tool_calls": false,
                "supports_reasoning_summaries": true,
                "supports_search_tool": false,
                "truncation_policy": { "mode": "bytes", "limit": 10000 },
                "upgrade": null,
                "visibility": "list"
            })
        })
        .collect::<Vec<_>>();

    json!({ "models": models })
}

fn codex_reasoning_description(effort: &str) -> &'static str {
    match effort {
        "low" => "Fast responses with lighter reasoning",
        "medium" => "Balances speed and reasoning depth for everyday tasks",
        "high" => "Greater reasoning depth for complex problems",
        "xhigh" => "Extra high reasoning depth for complex problems",
        "max" => "Maximum reasoning depth for the hardest problems",
        "ultra" => "Maximum reasoning with automatic task delegation",
        _ => "Reasoning effort",
    }
}

pub(crate) fn requested_model_from_body(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string)
        })
}

pub(crate) fn parse_model_capability(config_json: &str) -> ModelCapability {
    let mappings = serde_json::from_str::<Value>(config_json)
        .ok()
        .map(|value| parse_model_capability_value(&value))
        .map(|capability| capability.mappings)
        .unwrap_or_default();

    ModelCapability { mappings }
}

pub(crate) fn parse_model_capability_value(config: &Value) -> ModelCapability {
    let mappings = config
        .get("model_mappings")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ModelMapping>>(value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|mapping| {
            !is_placeholder_model(&mapping.from) && !is_placeholder_model(&mapping.to)
        })
        .collect();

    ModelCapability { mappings }
}

pub(crate) fn supports_requested_model(
    platform: &str,
    capability: &ModelCapability,
    requested_model: Option<&str>,
) -> bool {
    let Some(requested_model) = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
    else {
        return true;
    };

    if capability.mappings.is_empty() {
        return default_client_models(platform)
            .iter()
            .any(|model| model.eq_ignore_ascii_case(requested_model));
    }

    capability.mappings.iter().any(|mapping| {
        is_fallback_mapping(mapping) || model_mapping_matches(&mapping.from, requested_model)
    })
}

/// Picks the upstream model for a request: the first *specific* match wins, and
/// the fallback entry is consulted only when nothing specific matched. Two
/// passes rather than one `.find()` so the fallback loses regardless of where it
/// sits in the array — a hand-edited config can put it first.
pub(crate) fn resolve_mapping_target<'a>(
    mappings: &'a [ModelMapping],
    requested_model: &str,
) -> Option<&'a str> {
    mappings
        .iter()
        .find(|mapping| {
            !is_fallback_mapping(mapping) && model_mapping_matches(&mapping.from, requested_model)
        })
        .or_else(|| mappings.iter().find(|mapping| is_fallback_mapping(mapping)))
        .map(|mapping| mapping.to.as_str())
}

/// The key a `(account, model)` failure state is recorded under.
///
/// For `api` accounts this is the upstream model the request is rewritten to, so
/// a relay that rate-limits one upstream model parks exactly that one — and a
/// catch-all mapping funnels every client alias onto a single key instead of
/// letting each alias hit the wall separately.
///
/// `official` accounts never get their model rewritten
/// (`build_official_upstream_request`), so their key is the requested name.
pub(crate) fn model_state_key(
    platform: &str,
    capability: &ModelCapability,
    kind: &str,
    requested_model: &str,
) -> String {
    let requested = strip_one_m_suffix_for_route_lookup(requested_model);
    if kind == "official" {
        return requested.to_string();
    }
    let _ = platform;
    resolve_mapping_target(&capability.mappings, requested)
        .map(|target| strip_one_m_suffix_for_route_lookup(target).to_string())
        .unwrap_or_else(|| requested.to_string())
}

/// Map an upstream model key back to a client-facing alias, for places that must
/// speak the request vocabulary (the model test takes `mapping.from`).
pub(crate) fn alias_for_model_key(capability: &ModelCapability, model_key: &str) -> Option<String> {
    aliases_for_model_key(capability, model_key)
        .into_iter()
        .next()
}

/// Every client-facing alias pointing at this upstream model. A relay config can
/// route two aliases to one upstream model, and the UI shows them all so users
/// recognise the row by the name they typed.
pub(crate) fn aliases_for_model_key(capability: &ModelCapability, model_key: &str) -> Vec<String> {
    capability
        .mappings
        .iter()
        .filter(|mapping| {
            !is_fallback_mapping(mapping)
                && strip_one_m_suffix_for_route_lookup(&mapping.to) == model_key
        })
        .map(|mapping| mapping.from.trim().to_string())
        .collect()
}

/// Every model key this account could ever produce. Used as the denominator when
/// deciding whether an account-level escalation is due, and to list models the
/// user may pause before any of them has failed.
pub(crate) fn known_upstream_models(
    platform: &str,
    capability: &ModelCapability,
    kind: &str,
) -> Vec<String> {
    if kind == "official" || capability.mappings.is_empty() {
        return default_client_models(platform)
            .iter()
            .map(|model| (*model).to_string())
            .collect();
    }

    let mut models = Vec::new();
    for mapping in &capability.mappings {
        let target = strip_one_m_suffix_for_route_lookup(&mapping.to);
        if target.is_empty() || models.iter().any(|model| model == target) {
            continue;
        }
        models.push(target.to_string());
    }
    models
}

pub(crate) fn advertised_model_ids(
    platform: &str,
    capabilities: &[ModelCapability],
) -> Vec<String> {
    advertised_model_catalog_entries(platform, capabilities)
        .into_iter()
        .map(|model| model.id)
        .collect()
}

pub(crate) fn advertised_model_catalog_entries(
    platform: &str,
    capabilities: &[ModelCapability],
) -> Vec<AdvertisedModel> {
    let mut models = Vec::new();
    let mut seen = HashSet::new();

    // A fallback-carrying account accepts any model, so it advertises the
    // platform baseline exactly like an empty-mapping wildcard does.
    if capabilities.iter().any(|capability| {
        capability.mappings.is_empty() || capability.mappings.iter().any(is_fallback_mapping)
    }) {
        for model in default_client_models(platform) {
            // A baseline model is forwarded unrewritten, so it is its own
            // upstream.
            push_unique_model(
                &mut models,
                &mut seen,
                ModelContribution {
                    id: model,
                    description: model,
                    upstream_model: model,
                    context_window: None,
                    reasoning_levels: None,
                },
            );
        }
    }

    for capability in capabilities {
        for mapping in &capability.mappings {
            // The catch-all sentinel is not a model id — advertising it would put
            // `FALLBACK_MODEL_ALIAS` in front of users as if it were something
            // they could pick. Its baseline contribution is handled above.
            if is_fallback_mapping(mapping) {
                continue;
            }
            let from = mapping.from.trim();
            let to = mapping.to.trim();
            let description = if from.eq_ignore_ascii_case(to) {
                from.to_string()
            } else {
                format!("映射的上游模型：{to}")
            };
            let contribution = ModelContribution {
                id: from,
                description: &description,
                upstream_model: to,
                context_window: mapping.context_window,
                reasoning_levels: mapping.reasoning_levels.as_deref(),
            };
            push_unique_model(&mut models, &mut seen, contribution);

            if platform == "claude" && mapping.supports_1m == Some(true) {
                let base = strip_one_m_suffix_for_route_lookup(&mapping.from);
                if is_claude_route_model(base) {
                    let one_m = format!("{base}[1m]");
                    push_unique_model(
                        &mut models,
                        &mut seen,
                        ModelContribution {
                            id: &one_m,
                            ..contribution
                        },
                    );
                }
            }
        }
    }

    models
}

fn default_client_models(platform: &str) -> &'static [&'static str] {
    match platform {
        "codex" => &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"],
        "claude" => &[
            "claude-sonnet-alias",
            "claude-opus-alias",
            "claude-fable-alias",
            "claude-haiku-alias",
        ],
        "gemini" => &["gemini-2.5-flash"],
        "grok" => &["grok-4.5"],
        _ => &[],
    }
}

/// One account's claim on a model id, before the ids are deduped across the
/// pool.
#[derive(Clone, Copy)]
struct ModelContribution<'a> {
    id: &'a str,
    description: &'a str,
    upstream_model: &'a str,
    context_window: Option<u32>,
    reasoning_levels: Option<&'a [String]>,
}

fn push_unique_model(
    models: &mut Vec<AdvertisedModel>,
    seen: &mut HashSet<String>,
    contribution: ModelContribution<'_>,
) {
    let trimmed = contribution.id.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_ascii_lowercase();
    if seen.insert(key) {
        models.push(AdvertisedModel {
            id: trimmed.to_string(),
            description: contribution.description.to_string(),
            upstream_model: contribution.upstream_model.trim().to_string(),
            context_window: contribution.context_window,
            reasoning_levels: contribution.reasoning_levels.map(<[String]>::to_vec),
        });
    } else if let Some(existing) = models
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(trimmed))
    {
        if existing.description == existing.id && contribution.description != trimmed {
            existing.description = contribution.description.to_string();
        }
        // Two accounts can advertise one alias; the baseline contributes it with
        // no overrides at all. Let the first account that actually declares a
        // value fill the gap rather than leaving the catalog on the default.
        //
        // `upstream_model` is deliberately not merged: accounts that disagree
        // about where one alias points cannot be described by a single catalog
        // entry anyway, and the first claim is the one the description already
        // names.
        if existing.context_window.is_none() {
            existing.context_window = contribution.context_window;
        }
        if existing.reasoning_levels.is_none() {
            existing.reasoning_levels = contribution.reasoning_levels.map(<[String]>::to_vec);
        }
    }
}

fn is_placeholder_model(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value == "upstream-model"
}

fn model_mapping_matches(mapping_from: &str, requested_model: &str) -> bool {
    let mapping_from = mapping_from.trim();
    let requested_model = requested_model.trim();
    if mapping_from == requested_model {
        return true;
    }

    match (
        claude_route_lookup_model(mapping_from),
        claude_route_lookup_model(requested_model),
    ) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn claude_route_lookup_model(model: &str) -> Option<&str> {
    let stripped = strip_one_m_suffix_for_route_lookup(model);
    if is_claude_route_model(stripped) {
        Some(stripped)
    } else {
        None
    }
}

fn strip_one_m_suffix_for_route_lookup(model: &str) -> &str {
    const ONE_M_CONTEXT_MARKER: &str = "[1m]";
    let trimmed = model.trim();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    trimmed
}

fn is_claude_route_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.starts_with("claude-") || lower.starts_with("anthropic/claude-")
}

#[cfg(test)]
mod tests {
    use super::{
        advertised_model_catalog_entries, advertised_model_ids, alias_for_model_key,
        codex_default_context_window, codex_effective_context_window, codex_model_catalog_payload,
        codex_reasoning_levels, codex_reasoning_metadata, codex_reasoning_profile,
        known_upstream_models, model_state_key, parse_model_capability, requested_model_from_body,
        resolve_mapping_target, supports_requested_model, ModelCapability,
    };
    use crate::models::route_credential::{ModelMapping, FALLBACK_MODEL_ALIAS};

    #[test]
    fn codex_baseline_models_use_distinct_reasoning_profiles() {
        let sol = codex_reasoning_profile("gpt-5.6-sol");
        assert_eq!(sol.default_level, "low");
        assert_eq!(
            sol.levels,
            &["low", "medium", "high", "xhigh", "max", "ultra"]
        );

        let terra = codex_reasoning_profile("gpt-5.6-terra");
        assert_eq!(terra.default_level, "medium");
        assert_eq!(terra.levels, sol.levels);

        let luna = codex_reasoning_profile("gpt-5.6-luna");
        assert_eq!(luna.levels, &["low", "medium", "high", "xhigh", "max"]);

        let gpt_55 = codex_reasoning_profile("gpt-5.5");
        assert_eq!(gpt_55.levels, &["low", "medium", "high", "xhigh"]);
    }

    #[test]
    fn requested_model_reads_only_a_non_empty_top_level_model() {
        assert_eq!(
            requested_model_from_body(br#"{"model":"gpt-5.6-sol","input":"hi"}"#),
            Some("gpt-5.6-sol".to_string())
        );
        assert_eq!(
            requested_model_from_body(br#"{"nested":{"model":"gpt-5"}}"#),
            None
        );
        assert_eq!(requested_model_from_body(br#"{"model":""}"#), None);
        assert_eq!(requested_model_from_body(b"not-json"), None);
    }

    #[test]
    fn empty_mappings_are_wildcard_and_non_empty_mappings_are_restricted() {
        let wildcard = parse_model_capability(r#"{"model_mappings":[]}"#);
        let limited = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
        );

        assert!(supports_requested_model(
            "codex",
            &wildcard,
            Some("gpt-5.6-luna")
        ));
        assert!(!supports_requested_model(
            "codex",
            &wildcard,
            Some("deepseek")
        ));
        assert!(supports_requested_model(
            "codex",
            &limited,
            Some("gpt-5.6-sol")
        ));
        assert!(!supports_requested_model(
            "codex",
            &limited,
            Some("gpt-5.6-luna")
        ));
        assert!(supports_requested_model("codex", &limited, None));
    }

    #[test]
    fn codex_advertised_models_add_baseline_only_for_wildcards() {
        let wildcard = parse_model_capability(r#"{"model_mappings":[]}"#);
        let sol = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"},{"from":"custom","to":"custom-upstream"}]}"#,
        );
        let limited = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
        );

        assert_eq!(
            advertised_model_ids("codex", &[wildcard, sol]),
            vec![
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
                "custom"
            ]
        );
        assert_eq!(
            advertised_model_ids("codex", &[limited]),
            vec!["gpt-5.6-sol"]
        );
    }

    #[test]
    fn codex_catalog_describes_upstream_model_for_mappings() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"deepseek-v4-flash-0731"},{"from":"gpt-5.5","to":"gpt-5.5"}]}"#,
        );
        let catalog = codex_model_catalog_payload(&[capability]);
        let models = catalog["models"].as_array().expect("catalog models");

        assert_eq!(models[0]["slug"], "gpt-5.6-sol");
        assert_eq!(
            models[0]["description"],
            "映射的上游模型：deepseek-v4-flash-0731"
        );
        assert_eq!(models[1]["slug"], "gpt-5.5");
        assert_eq!(models[1]["description"], "gpt-5.5");
    }

    #[test]
    fn codex_reasoning_levels_fall_back_to_the_baseline_profile() {
        // An absent list is what every mapping written before the field existed
        // carries, so it has to keep meaning "the profile for this model id".
        assert_eq!(
            codex_reasoning_levels("gpt-5.6-luna", None),
            vec!["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(
            codex_reasoning_levels("some-relay-model", None),
            vec!["low", "medium", "high"]
        );
        // An empty list is the same statement as no list: the UI writes one when
        // the user unticks every box, and an empty menu would strand the client.
        assert_eq!(
            codex_reasoning_levels("gpt-5.5", Some(&[])),
            vec!["low", "medium", "high", "xhigh"]
        );
    }

    #[test]
    fn codex_reasoning_levels_normalize_a_custom_list() {
        let declared = [
            " High ".to_string(),
            "high".to_string(),
            String::new(),
            "ultra".to_string(),
        ];
        assert_eq!(
            codex_reasoning_levels("gpt-5.6-sol", Some(&declared)),
            vec!["high", "ultra"]
        );
    }

    #[test]
    fn codex_reasoning_metadata_keeps_the_default_inside_a_custom_list() {
        // gpt-5.5 defaults to medium; a list without it must not preselect an
        // effort the client was just told is unavailable.
        let (levels, default_level) =
            codex_reasoning_metadata("gpt-5.5", Some(&["xhigh".to_string(), "max".to_string()]));
        assert_eq!(
            levels
                .iter()
                .filter_map(|level| level["effort"].as_str())
                .collect::<Vec<_>>(),
            vec!["xhigh", "max"]
        );
        assert_eq!(default_level, "xhigh");

        let (_, kept) =
            codex_reasoning_metadata("gpt-5.5", Some(&["high".to_string(), "medium".to_string()]));
        assert_eq!(kept, "medium");
    }

    #[test]
    fn codex_catalog_honours_per_mapping_context_and_reasoning_overrides() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[
                {"from":"gpt-5.6-sol","to":"upstream-sol","context_window":400000,"reasoning_levels":["medium","max"]},
                {"from":"glm-5.3","to":"upstream-glm"}
            ]}"#,
        );
        let catalog = codex_model_catalog_payload(&[capability]);
        let models = catalog["models"].as_array().expect("catalog models");

        assert_eq!(models[0]["context_window"], 400_000);
        assert_eq!(models[0]["max_context_window"], 400_000);
        assert_eq!(
            models[0]["supported_reasoning_levels"]
                .as_array()
                .expect("levels")
                .iter()
                .filter_map(|level| level["effort"].as_str())
                .collect::<Vec<_>>(),
            vec!["medium", "max"]
        );
        assert_eq!(models[0]["default_reasoning_level"], "medium");

        // An untouched row keeps the shipped defaults.
        assert_eq!(models[1]["context_window"], 256_000);
        assert_eq!(
            models[1]["supported_reasoning_levels"]
                .as_array()
                .expect("levels")
                .len(),
            3
        );
    }

    #[test]
    fn a_declaring_account_fills_the_overrides_a_wildcard_left_empty() {
        // The wildcard contributes gpt-5.6-sol from the baseline with no
        // overrides; the mapping that follows must still get its own numbers in.
        let wildcard = parse_model_capability(r#"{"model_mappings":[]}"#);
        let declaring = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol","context_window":256000,"reasoning_levels":["high"]}]}"#,
        );
        let entries = advertised_model_catalog_entries("codex", &[wildcard, declaring]);
        let sol = entries
            .iter()
            .find(|entry| entry.id == "gpt-5.6-sol")
            .expect("sol entry");

        assert_eq!(sol.context_window, Some(256_000));
        assert_eq!(
            sol.reasoning_levels.as_deref(),
            Some(&["high".to_string()][..])
        );
    }

    #[test]
    fn known_one_m_upstream_families_default_to_one_m() {
        for upstream in [
            "deepseek-v4",
            "deepseek-v4-flash-0731",
            "glm-5.2",
            "glm-5.3-air",
            "qwen-3.8-plus",
            "kimi-k3-turbo",
            // Relays that namespace by vendor must resolve the same way.
            "z-ai/glm-5.3",
            "moonshotai/kimi-k3",
            // Case is the relay's choice, not a different model.
            "DeepSeek-V4-Flash",
        ] {
            assert_eq!(
                codex_default_context_window(upstream),
                1_000_000,
                "upstream={upstream}"
            );
        }
    }

    #[test]
    fn other_upstream_models_default_to_the_generic_window() {
        for upstream in [
            "gpt-5.6-sol",
            // An older generation of the same family is not in the table.
            "deepseek-v3-chat",
            "glm-5.1",
            "qwen-3.7",
            "kimi-k2",
            // A vendor path whose model half does not match either.
            "openai/gpt-5.5",
            "",
        ] {
            assert_eq!(
                codex_default_context_window(upstream),
                256_000,
                "upstream={upstream}"
            );
        }
    }

    #[test]
    fn a_declared_window_outranks_the_upstream_default() {
        assert_eq!(
            codex_effective_context_window(Some(400_000), "deepseek-v4-flash"),
            400_000
        );
        // Zero is not a window anything could serve, so the default still applies.
        assert_eq!(
            codex_effective_context_window(Some(0), "deepseek-v4-flash"),
            1_000_000
        );
        assert_eq!(codex_effective_context_window(None, "gpt-5.5"), 256_000);
    }

    #[test]
    fn the_catalog_reads_the_default_off_the_upstream_not_the_alias() {
        // The alias is a plain gpt id; only the mapped-to name says 1M. Reading
        // the alias instead would advertise 256K for a model serving 1M.
        let capability = parse_model_capability(
            r#"{"model_mappings":[
                {"from":"gpt-5.6-sol","to":"deepseek-v4-flash-0731"},
                {"from":"gpt-5.5","to":"gpt-5.5"}
            ]}"#,
        );
        let catalog = codex_model_catalog_payload(&[capability]);
        let models = catalog["models"].as_array().expect("catalog models");

        assert_eq!(models[0]["slug"], "gpt-5.6-sol");
        assert_eq!(models[0]["context_window"], 1_000_000);
        assert_eq!(models[0]["max_context_window"], 1_000_000);
        assert_eq!(models[1]["slug"], "gpt-5.5");
        assert_eq!(models[1]["context_window"], 256_000);
    }

    #[test]
    fn a_passthrough_one_m_mapping_is_advertised_at_one_m() {
        // What the relay presets actually produce: from == to == the real model.
        let capability =
            parse_model_capability(r#"{"model_mappings":[{"from":"glm-5.3","to":"glm-5.3"}]}"#);
        let catalog = codex_model_catalog_payload(&[capability]);

        assert_eq!(catalog["models"][0]["context_window"], 1_000_000);
    }

    #[test]
    fn claude_advertised_models_expand_one_m_and_dedupe_case_insensitively() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"sonnet","supports_1m":true},{"from":"CLAUDE-SONNET-ALIAS","to":"other"}]}"#,
        );

        assert_eq!(
            advertised_model_ids("claude", &[capability]),
            vec!["claude-sonnet-alias", "claude-sonnet-alias[1m]"]
        );
    }

    #[test]
    fn fallback_mapping_makes_any_model_supported() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"x"},{"from":"claude-model","to":"y"}]}"#,
        );

        assert!(supports_requested_model(
            "claude",
            &capability,
            Some("claude-opus-alias")
        ));
        assert!(supports_requested_model(
            "claude",
            &capability,
            Some("anything-at-all")
        ));
        assert!(supports_requested_model(
            "claude",
            &capability,
            Some("claude-sonnet-alias")
        ));
        assert!(supports_requested_model("claude", &capability, None));
    }

    #[test]
    fn fallback_mapping_is_never_advertised_but_contributes_baseline() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"x"},{"from":"claude-model","to":"y"}]}"#,
        );
        let advertised = advertised_model_ids("claude", &[capability]);

        // A fallback account genuinely accepts anything, so it advertises the
        // platform baseline just like an empty-mapping wildcard would.
        assert_eq!(
            advertised,
            vec![
                "claude-sonnet-alias",
                "claude-opus-alias",
                "claude-fable-alias",
                "claude-haiku-alias"
            ]
        );
        assert!(!advertised.iter().any(|model| model == FALLBACK_MODEL_ALIAS));
    }

    #[test]
    fn fallback_only_capability_advertises_baseline_and_matches_everything() {
        let capability =
            parse_model_capability(r#"{"model_mappings":[{"from":"claude-model","to":"y"}]}"#);

        assert_eq!(
            advertised_model_ids("codex", &[capability.clone()]),
            vec!["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"]
        );
        assert!(supports_requested_model(
            "codex",
            &capability,
            Some("deepseek")
        ));
    }

    #[test]
    fn resolve_mapping_target_prefers_specific_entries_regardless_of_order() {
        // Fallback deliberately sits FIRST: a single-pass `.find()` would let it
        // swallow every request, which is the bug this ordering guards against.
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-model","to":"fallback-upstream"},{"from":"claude-sonnet-alias","to":"sonnet-upstream","supports_1m":true}]}"#,
        );
        let mappings = &capability.mappings;

        assert_eq!(
            resolve_mapping_target(mappings, "claude-sonnet-alias"),
            Some("sonnet-upstream")
        );
        assert_eq!(
            resolve_mapping_target(mappings, "claude-haiku-alias"),
            Some("fallback-upstream")
        );
        assert_eq!(
            resolve_mapping_target(mappings, "claude-sonnet-alias[1m]"),
            Some("sonnet-upstream")
        );
    }

    #[test]
    fn resolve_mapping_target_returns_none_without_a_fallback() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"sonnet-upstream"}]}"#,
        );

        assert_eq!(
            resolve_mapping_target(&capability.mappings, "claude-haiku-alias"),
            None
        );
    }

    #[test]
    fn a_one_m_catch_all_row_is_not_advertised_under_either_name() {
        // The catch-all alias now shares the `claude-` prefix with the real role
        // aliases, so `is_claude_route_model` accepts it and no longer acts as a
        // second line of defense in the 1M-variant expansion. Only the
        // `is_fallback_mapping` guard keeps `claude-model[1m]` out of the catalog.
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"sonnet-upstream"},{"from":"claude-model","to":"fallback-upstream","supports_1m":true}]}"#,
        );

        let advertised = advertised_model_ids("claude", &[capability.clone()]);
        assert!(
            !advertised
                .iter()
                .any(|model| model.to_ascii_lowercase().starts_with(FALLBACK_MODEL_ALIAS)),
            "advertised={advertised:?}"
        );

        // And the alias still routes when a client requests it verbatim —
        // `ANTHROPIC_MODEL` is written as exactly this value.
        for requested in [FALLBACK_MODEL_ALIAS, "claude-model[1M]"] {
            assert!(supports_requested_model(
                "claude",
                &capability,
                Some(requested)
            ));
            assert_eq!(
                resolve_mapping_target(&capability.mappings, requested),
                Some("fallback-upstream"),
                "requested={requested}"
            );
        }
    }

    #[test]
    fn sentinels_are_not_placeholder_models() {
        // `is_placeholder_model` must keep ignoring the route sentinels: filtering
        // them here would silently delete both features at parse time.
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-model","to":"y"},{"from":"claude-subagent","to":"z"}]}"#,
        );

        assert_eq!(capability.mappings.len(), 2);
    }

    #[test]
    fn catalog_entries_are_shared_with_client_config_writers() {
        let mapping = ModelMapping {
            from: "gpt-5.6-sol".to_string(),
            to: "gpt-5.6-sol".to_string(),
            label: None,
            supports_1m: None,
            ..Default::default()
        };
        let capability = ModelCapability {
            mappings: vec![mapping],
        };

        // Same source of truth the Codex catalog uses, so a client config and
        // the catalog can never advertise different models.
        let entries = advertised_model_catalog_entries("codex", &[capability]);

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.6-sol"]
        );
    }

    #[test]
    fn subagent_alias_is_advertised_and_matched_without_special_casing() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"claude-subagent","to":"provider-haiku"}]}"#,
        );

        assert_eq!(
            advertised_model_ids("claude", &[capability.clone()]),
            vec!["claude-subagent"]
        );
        assert!(supports_requested_model(
            "claude",
            &capability,
            Some("claude-subagent")
        ));
        assert_eq!(
            resolve_mapping_target(&capability.mappings, "claude-subagent"),
            Some("provider-haiku")
        );
    }

    #[test]
    fn model_state_key_uses_the_mapped_upstream_model_for_api_accounts() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            model_state_key("codex", &capability, "api", "gpt-5.6-sol"),
            "upstream-sol"
        );
    }

    #[test]
    fn model_state_key_collapses_catch_all_aliases_onto_one_key() {
        let capability = parse_model_capability(&format!(
            r#"{{"model_mappings":[{{"from":"{FALLBACK_MODEL_ALIAS}","to":"upstream-any"}}]}}"#
        ));
        // Every client-side name funnels into the same upstream model, so one
        // failure parks it for all of them instead of once per alias.
        assert_eq!(
            model_state_key("claude", &capability, "api", "claude-sonnet-alias"),
            "upstream-any"
        );
        assert_eq!(
            model_state_key("claude", &capability, "api", "whatever-else"),
            "upstream-any"
        );
    }

    #[test]
    fn model_state_key_keeps_the_requested_name_for_official_and_empty_mappings() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        assert_eq!(
            model_state_key("codex", &empty, "api", "gpt-5.6-sol"),
            "gpt-5.6-sol"
        );

        // build_official_upstream_request never rewrites the model, so an
        // official account's key must be the name the client sent.
        let official = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            model_state_key("codex", &official, "official", "gpt-5.6-sol"),
            "gpt-5.6-sol"
        );
    }

    #[test]
    fn model_state_key_strips_the_one_m_suffix() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        // Same upstream model, only a different beta header — one cooldown.
        assert_eq!(
            model_state_key("claude", &empty, "api", "claude-sonnet-alias[1m]"),
            model_state_key("claude", &empty, "api", "claude-sonnet-alias")
        );
    }

    #[test]
    fn known_upstream_models_returns_the_platform_baseline_without_mappings() {
        let empty = parse_model_capability(r#"{"model_mappings":[]}"#);
        let models = known_upstream_models("codex", &empty, "api");
        assert!(models.contains(&"gpt-5.6-sol".to_string()));
        assert!(models.contains(&"gpt-5.5".to_string()));
    }

    #[test]
    fn known_upstream_models_dedupes_targets_and_includes_the_catch_all_target() {
        let capability = parse_model_capability(&format!(
            r#"{{"model_mappings":[
                {{"from":"gpt-5.6-sol","to":"upstream-a"}},
                {{"from":"glm-5.3","to":"upstream-a"}},
                {{"from":"gpt-5.5","to":"upstream-b"}},
                {{"from":"{FALLBACK_MODEL_ALIAS}","to":"upstream-any"}}
            ]}}"#
        ));
        let mut models = known_upstream_models("codex", &capability, "api");
        models.sort();
        assert_eq!(models, vec!["upstream-a", "upstream-any", "upstream-b"]);
    }

    #[test]
    fn known_upstream_models_ignores_mappings_for_official_accounts() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        let models = known_upstream_models("codex", &capability, "official");
        assert!(models.contains(&"gpt-5.6-sol".to_string()));
        assert!(!models.contains(&"upstream-sol".to_string()));
    }

    #[test]
    fn alias_for_model_key_maps_upstream_names_back_to_client_aliases() {
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"upstream-sol"}]}"#,
        );
        assert_eq!(
            alias_for_model_key(&capability, "upstream-sol").as_deref(),
            Some("gpt-5.6-sol")
        );
        assert!(alias_for_model_key(&capability, "unknown").is_none());
    }
}
