use serde_json::{json, Value};

pub struct RoutePreviewService;

impl RoutePreviewService {
    pub fn generate(
        platform: &str,
        kind: &str,
        secret_payload_json: &str,
        config_json: &str,
    ) -> String {
        let secret: Value = serde_json::from_str(secret_payload_json).unwrap_or(Value::Null);
        let config: Value = serde_json::from_str(config_json).unwrap_or(Value::Null);

        match platform {
            "codex" => json!({
                "auth_json": preview_auth(kind, &secret),
                "config_toml": codex_toml(&config),
            })
            .to_string(),
            "claude" | "gemini" => json!({
                "settings_json": json!({
                    "aiSwitch": {
                        "kind": kind,
                        "baseUrl": config.get("base_url").and_then(Value::as_str),
                        "interfaceFormat": config.get("interface_format").and_then(Value::as_str),
                        "apiKeyField": config.get("api_key_field").and_then(Value::as_str),
                    }
                }).to_string()
            })
            .to_string(),
            _ => "{}".to_string(),
        }
    }
}

fn preview_auth(kind: &str, secret: &Value) -> Value {
    if kind == "api" {
        json!({ "api_key": secret.get("api_key").and_then(Value::as_str).unwrap_or("<api-key>") })
    } else {
        json!({
            "access_token": secret.get("access_token").and_then(Value::as_str).unwrap_or("<access-token>"),
            "refresh_token": secret.get("refresh_token").and_then(Value::as_str),
        })
    }
}

fn codex_toml(config: &Value) -> String {
    let base_url = config
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or("http://127.0.0.1:43111/v1");
    format!("model_provider = \"ai-switch\"\n\n[model_providers.ai-switch]\nbase_url = \"{base_url}\"\n")
}
