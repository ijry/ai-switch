use crate::models::route_credential::{is_fallback_mapping, ModelMapping};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelCapability {
    pub(crate) mappings: Vec<ModelMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdvertisedModel {
    id: String,
    description: String,
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

pub(crate) fn codex_reasoning_metadata(model: &str) -> (Vec<Value>, &'static str) {
    let profile = codex_reasoning_profile(model);
    let levels = profile
        .levels
        .iter()
        .map(|effort| {
            json!({
                "effort": effort,
                "description": codex_reasoning_description(effort),
            })
        })
        .collect();
    (levels, profile.default_level)
}

pub(crate) fn codex_model_catalog_payload(capabilities: &[ModelCapability]) -> Value {
    let models = advertised_model_catalog_entries("codex", capabilities)
        .into_iter()
        .enumerate()
        .map(|(index, model)| {
            let (supported_reasoning_levels, default_reasoning_level) =
                codex_reasoning_metadata(&model.id);
            json!({
                "additional_speed_tiers": [],
                "availability_nux": null,
                "base_instructions": "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals.",
                "context_window": 128000,
                "default_reasoning_level": default_reasoning_level,
                "default_reasoning_summary": "none",
                "description": model.description,
                "display_name": model.id,
                "effective_context_window_percent": 95,
                "experimental_supported_tools": [],
                "input_modalities": ["text", "image"],
                "max_context_window": 128000,
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

pub(crate) fn advertised_model_ids(
    platform: &str,
    capabilities: &[ModelCapability],
) -> Vec<String> {
    advertised_model_catalog_entries(platform, capabilities)
        .into_iter()
        .map(|model| model.id)
        .collect()
}

fn advertised_model_catalog_entries(
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
            push_unique_model(&mut models, &mut seen, model, model);
        }
    }

    for capability in capabilities {
        for mapping in &capability.mappings {
            // The catch-all sentinel is not a model id — advertising it would put
            // "*" in front of users. Its baseline contribution is handled above.
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
            push_unique_model(&mut models, &mut seen, from, &description);

            if platform == "claude" && mapping.supports_1m == Some(true) {
                let base = strip_one_m_suffix_for_route_lookup(&mapping.from);
                if is_claude_route_model(base) {
                    push_unique_model(&mut models, &mut seen, &format!("{base}[1m]"), &description);
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

fn push_unique_model(
    models: &mut Vec<AdvertisedModel>,
    seen: &mut HashSet<String>,
    model: &str,
    description: &str,
) {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_ascii_lowercase();
    if seen.insert(key) {
        models.push(AdvertisedModel {
            id: trimmed.to_string(),
            description: description.to_string(),
        });
    } else if let Some(existing) = models
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(trimmed))
    {
        if existing.description == existing.id && description != trimmed {
            existing.description = description.to_string();
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
        advertised_model_ids, codex_model_catalog_payload, codex_reasoning_profile,
        parse_model_capability, requested_model_from_body, resolve_mapping_target,
        supports_requested_model,
    };

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
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"x"},{"from":"*","to":"y"}]}"#,
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
            r#"{"model_mappings":[{"from":"claude-sonnet-alias","to":"x"},{"from":"*","to":"y"}]}"#,
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
        assert!(!advertised.iter().any(|model| model == "*"));
    }

    #[test]
    fn fallback_only_capability_advertises_baseline_and_matches_everything() {
        let capability = parse_model_capability(r#"{"model_mappings":[{"from":"*","to":"y"}]}"#);

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
            r#"{"model_mappings":[{"from":"*","to":"fallback-upstream"},{"from":"claude-sonnet-alias","to":"sonnet-upstream","supports_1m":true}]}"#,
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
    fn sentinels_are_not_placeholder_models() {
        // `is_placeholder_model` must keep ignoring the route sentinels: filtering
        // them here would silently delete both features at parse time.
        let capability = parse_model_capability(
            r#"{"model_mappings":[{"from":"*","to":"y"},{"from":"claude-subagent","to":"z"}]}"#,
        );

        assert_eq!(capability.mappings.len(), 2);
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
}
