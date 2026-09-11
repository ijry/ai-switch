use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
struct ModelImageInputRules {
    #[serde(rename = "unknownDefault")]
    unknown_default: bool,
    rules: Vec<ModelImageInputRule>,
}

#[derive(Debug, Deserialize)]
struct ModelImageInputRule {
    r#match: RuleMatch,
    pattern: String,
    #[serde(rename = "supportsImageInput")]
    supports_image_input: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RuleMatch {
    Exact,
    Prefix,
    Contains,
}

const MODEL_IMAGE_INPUT_RULES_JSON: &str = include_str!("../../../src/modelCapabilities.json");

fn rules() -> &'static ModelImageInputRules {
    static RULES: OnceLock<ModelImageInputRules> = OnceLock::new();
    RULES.get_or_init(|| {
        serde_json::from_str(MODEL_IMAGE_INPUT_RULES_JSON).expect("model capability JSON is valid")
    })
}

fn normalize_model_id(model: &str) -> String {
    let trimmed = model.trim().to_ascii_lowercase();
    let segment = trimmed.rsplit('/').next().unwrap_or(trimmed.as_str());
    segment.replace('_', "-")
}

/// Built-in defaults for whether a model accepts images in normal chat input.
pub(crate) fn default_supports_image_input(model: &str) -> bool {
    let id = normalize_model_id(model);
    for rule in &rules().rules {
        let matched = match rule.r#match {
            RuleMatch::Exact => id == rule.pattern,
            RuleMatch::Prefix => id.starts_with(&rule.pattern),
            RuleMatch::Contains => id.contains(&rule.pattern),
        };
        if matched {
            return rule.supports_image_input;
        }
    }
    rules().unknown_default
}

#[cfg(test)]
mod tests {
    use super::default_supports_image_input;

    #[test]
    fn deepseek_v4_1_matches_by_prefix() {
        assert!(default_supports_image_input("deepseek-v4.1"));
        assert!(default_supports_image_input("deepseek-v4.1-preview"));
    }

    #[test]
    fn requested_family_defaults_are_applied() {
        assert!(default_supports_image_input("gpt-5.6-sol"));
        assert!(!default_supports_image_input("deepseek-v4-flash-0731"));
        assert!(!default_supports_image_input("glm-5.3"));
        assert!(default_supports_image_input("glm-5.2-flash"));
        assert!(default_supports_image_input("qwen3-vl-plus"));
        assert!(!default_supports_image_input("qwen3-plus"));
        assert!(!default_supports_image_input("unknown-model"));
    }
}
