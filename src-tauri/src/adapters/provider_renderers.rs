use crate::error::AppError;
use crate::models::provider::Provider;
use crate::models::target_app::TargetApp;
use serde::Serialize;
use serde_json::Value;

const SUPPORTED_TARGET_KEYS: [&str; 7] = [
    "claude_code",
    "claude_desktop",
    "codex",
    "gemini_cli",
    "opencode",
    "openclaw",
    "hermes",
];

#[derive(Serialize)]
struct SandboxConfig<'a> {
    schema: &'static str,
    target: SandboxTarget<'a>,
    provider: SandboxProvider<'a>,
    model_config: Value,
    target_options: Value,
    rendered_for: &'a str,
}

#[derive(Serialize)]
struct SandboxTarget<'a> {
    key: &'a str,
    display_name: &'a str,
}

#[derive(Serialize)]
struct SandboxProvider<'a> {
    id: &'a str,
    name: &'a str,
    kind: &'a str,
    base_url: Option<&'a str>,
    secret_ref: Option<&'a str>,
    secret_value: Option<&'static str>,
}

pub fn render_provider_sandbox_config(
    target: &TargetApp,
    provider: &Provider,
) -> Result<String, AppError> {
    if !SUPPORTED_TARGET_KEYS.contains(&target.key.as_str()) {
        return Err(AppError::Adapter {
            code: "adapter.target_not_supported",
            message: "Target app is not supported by the sandbox provider renderer".to_string(),
            details: Some(target.key.clone()),
            recoverable: false,
        });
    }

    let model_config = parse_json_object(
        &provider.model_config_json,
        "validation.provider_model_config_json",
        "Provider model configuration must be a JSON object",
    )?;
    let target_options = parse_json_object(
        &provider.target_options_json,
        "validation.provider_target_options_json",
        "Provider target options must be a JSON object",
    )?;
    let selected_target_options = target_options
        .as_object()
        .and_then(|object| object.get(&target.key))
        .cloned()
        .unwrap_or(target_options);

    let payload = SandboxConfig {
        schema: "ai-switch.provider-switch.sandbox.v1",
        target: SandboxTarget {
            key: &target.key,
            display_name: &target.display_name,
        },
        provider: SandboxProvider {
            id: &provider.id,
            name: &provider.name,
            kind: &provider.kind,
            base_url: provider.base_url.as_deref(),
            secret_ref: provider.secret_ref.as_deref(),
            secret_value: provider.secret_ref.as_ref().map(|_| "[redacted]"),
        },
        model_config,
        target_options: selected_target_options,
        rendered_for: &target.key,
    };

    serde_json::to_string_pretty(&payload).map_err(|err| AppError::Validation {
        code: "validation.provider_render_json",
        message: "Could not render provider sandbox config".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })
}

fn parse_json_object(raw: &str, code: &'static str, message: &str) -> Result<Value, AppError> {
    let value: Value = serde_json::from_str(raw).map_err(|err| AppError::Validation {
        code,
        message: message.to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    if !value.is_object() {
        return Err(AppError::Validation {
            code,
            message: message.to_string(),
            details: Some("Expected a JSON object".to_string()),
            recoverable: true,
        });
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::render_provider_sandbox_config;
    use crate::models::provider::Provider;
    use crate::models::target_app::TargetApp;
    use serde_json::Value;

    fn target(key: &str) -> TargetApp {
        TargetApp {
            id: format!("{key}-id"),
            key: key.to_string(),
            display_name: key.to_string(),
            enabled: 1,
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    fn provider() -> Provider {
        Provider {
            id: "provider-1".to_string(),
            name: "Acme Provider".to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{\"default\":\"gpt-4.1\"}".to_string(),
            target_options_json: "{\"codex\":{\"model\":\"gpt-4.1-mini\"},\"timeout\":30}"
                .to_string(),
            secret_ref: Some("secret://provider/acme".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn renders_all_default_targets_deterministically_and_redacts_secret() {
        let keys = [
            "claude_code",
            "claude_desktop",
            "codex",
            "gemini_cli",
            "opencode",
            "openclaw",
            "hermes",
        ];

        for key in keys {
            let first =
                render_provider_sandbox_config(&target(key), &provider()).expect("first render");
            let second =
                render_provider_sandbox_config(&target(key), &provider()).expect("second render");
            let value: Value = serde_json::from_str(&first).expect("json");

            assert_eq!(first, second);
            assert_eq!(value["schema"], "ai-switch.provider-switch.sandbox.v1");
            assert_eq!(value["target"]["key"], key);
            assert_eq!(value["provider"]["secret_ref"], "secret://provider/acme");
            assert_eq!(value["provider"]["secret_value"], "[redacted]");
            assert_eq!(value["rendered_for"], key);
        }
    }

    #[test]
    fn uses_target_specific_options_when_present() {
        let rendered =
            render_provider_sandbox_config(&target("codex"), &provider()).expect("render");
        let value: Value = serde_json::from_str(&rendered).expect("json");

        assert_eq!(value["target_options"]["model"], "gpt-4.1-mini");
        assert!(value["target_options"]["timeout"].is_null());
    }

    #[test]
    fn rejects_malformed_model_config_json() {
        let mut provider = provider();
        provider.model_config_json = "{".to_string();

        let error = render_provider_sandbox_config(&target("codex"), &provider).expect_err("error");

        assert_eq!(error.code(), "validation.provider_model_config_json");
    }

    #[test]
    fn rejects_malformed_target_options_json() {
        let mut provider = provider();
        provider.target_options_json = "{".to_string();

        let error = render_provider_sandbox_config(&target("codex"), &provider).expect_err("error");

        assert_eq!(error.code(), "validation.provider_target_options_json");
    }

    #[test]
    fn rejects_unsupported_target_key() {
        let error =
            render_provider_sandbox_config(&target("unknown"), &provider()).expect_err("error");

        assert_eq!(error.code(), "adapter.target_not_supported");
    }
}
