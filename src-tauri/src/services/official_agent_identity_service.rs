use base64::engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{Signer as _, SigningKey};
use serde_json::Value;
use std::collections::BTreeMap;

pub const CODEX_AGENT_IDENTITY_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentIdentityHeaders {
    pub authorization: String,
    pub chatgpt_account_id: String,
    pub is_fedramp_account: bool,
}

pub fn is_official_agent_identity_credential(secret: &Value, config: &Value) -> bool {
    auth_mode_is_agent_identity(secret, config)
        || string_from_any(secret, config, &[&["agent_private_key"], &["agentPrivateKey"]])
            .is_some()
}

pub fn resolve_agent_identity_headers(
    secret: &Value,
    config: &Value,
) -> Result<Option<AgentIdentityHeaders>, String> {
    if !is_official_agent_identity_credential(secret, config) {
        return Ok(None);
    }

    let agent_runtime_id = required_agent_identity_field(
        secret,
        config,
        "agent_runtime_id",
        &[&["agent_runtime_id"], &["agentRuntimeId"]],
    )?;
    let private_key = required_agent_identity_field(
        secret,
        config,
        "agent_private_key",
        &[&["agent_private_key"], &["agentPrivateKey"]],
    )?;
    let task_id =
        required_agent_identity_field(secret, config, "task_id", &[&["task_id"], &["taskId"]])?;
    let chatgpt_account_id = string_from_any(
        secret,
        config,
        &[
            &["account_id"],
            &["accountId"],
            &["chatgpt_account_id"],
            &["chatgptAccountId"],
        ],
    )
    .ok_or_else(|| "Agent identity credential is missing account_id".to_string())?;

    Ok(Some(AgentIdentityHeaders {
        authorization: authorization_header_for_agent_task(
            &agent_runtime_id,
            &private_key,
            &task_id,
        )?,
        chatgpt_account_id,
        is_fedramp_account: bool_from_any(
            secret,
            config,
            &[
                &["chatgpt_account_is_fedramp"],
                &["chatgptAccountIsFedramp"],
                &["is_fedramp_account"],
                &["isFedrampAccount"],
            ],
        )
        .unwrap_or(false),
    }))
}

fn required_agent_identity_field(
    secret: &Value,
    config: &Value,
    label: &str,
    paths: &[&[&str]],
) -> Result<String, String> {
    string_from_any(secret, config, paths)
        .ok_or_else(|| format!("Agent identity credential is missing {label}"))
}

fn authorization_header_for_agent_task(
    agent_runtime_id: &str,
    private_key_pkcs8_base64: &str,
    task_id: &str,
) -> Result<String, String> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let signature =
        sign_agent_assertion_payload(agent_runtime_id, private_key_pkcs8_base64, task_id, &timestamp)?;
    let payload = serde_json::to_vec(&BTreeMap::from([
        ("agent_runtime_id", agent_runtime_id),
        ("signature", signature.as_str()),
        ("task_id", task_id),
        ("timestamp", timestamp.as_str()),
    ]))
    .map_err(|err| format!("Could not serialize agent assertion: {err}"))?;
    Ok(format!("AgentAssertion {}", URL_SAFE_NO_PAD.encode(payload)))
}

fn sign_agent_assertion_payload(
    agent_runtime_id: &str,
    private_key_pkcs8_base64: &str,
    task_id: &str,
    timestamp: &str,
) -> Result<String, String> {
    let signing_key = signing_key_from_private_key_pkcs8_base64(private_key_pkcs8_base64)?;
    let payload = format!("{agent_runtime_id}:{task_id}:{timestamp}");
    Ok(BASE64_STANDARD.encode(signing_key.sign(payload.as_bytes()).to_bytes()))
}

fn signing_key_from_private_key_pkcs8_base64(
    private_key_pkcs8_base64: &str,
) -> Result<SigningKey, String> {
    let private_key = BASE64_STANDARD
        .decode(private_key_pkcs8_base64)
        .map_err(|err| format!("Agent identity private key is not valid base64: {err}"))?;
    SigningKey::from_pkcs8_der(&private_key)
        .map_err(|err| format!("Agent identity private key is not valid PKCS#8: {err}"))
}

fn auth_mode_is_agent_identity(secret: &Value, config: &Value) -> bool {
    string_from_any(
        secret,
        config,
        &[
            &["auth_mode"],
            &["authMode"],
            &["auth_kind"],
            &["authKind"],
            &["raw_type"],
        ],
    )
    .is_some_and(|value| {
        let normalized = value
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        normalized == "agentidentity"
    })
}

fn string_from_any(secret: &Value, config: &Value, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| string_at_path(secret, path))
        .or_else(|| paths.iter().find_map(|path| string_at_path(config, path)))
        .or_else(|| {
            config
                .get("raw")
                .and_then(|raw| paths.iter().find_map(|path| string_at_path(raw, path)))
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("credentials")
                    .and_then(|credentials| paths.iter().find_map(|path| string_at_path(credentials, path)))
            })
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("extra")
                    .and_then(|extra| paths.iter().find_map(|path| string_at_path(extra, path)))
            })
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("credentials")
                    .and_then(|credentials| credentials.get("extra"))
                    .and_then(|extra| paths.iter().find_map(|path| string_at_path(extra, path)))
            })
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("credentials")
                    .and_then(|credentials| credentials.get("live_identity"))
                    .and_then(|identity| paths.iter().find_map(|path| string_at_path(identity, path)))
            })
        })
}

fn bool_from_any(secret: &Value, config: &Value, paths: &[&[&str]]) -> Option<bool> {
    paths
        .iter()
        .find_map(|path| bool_at_path(secret, path))
        .or_else(|| paths.iter().find_map(|path| bool_at_path(config, path)))
        .or_else(|| {
            config
                .get("raw")
                .and_then(|raw| paths.iter().find_map(|path| bool_at_path(raw, path)))
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("credentials")
                    .and_then(|credentials| paths.iter().find_map(|path| bool_at_path(credentials, path)))
            })
        })
        .or_else(|| {
            config.get("raw").and_then(|raw| {
                raw.get("credentials")
                    .and_then(|credentials| credentials.get("live_identity"))
                    .and_then(|identity| paths.iter().find_map(|path| bool_at_path(identity, path)))
            })
        })
}

fn string_at_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn bool_at_path(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    match current {
        Value::Bool(value) => Some(*value),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use ed25519_dalek::{Signature, Verifier as _};
    use serde_json::json;

    fn test_private_key() -> (SigningKey, String) {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let private_key = signing_key.to_pkcs8_der().expect("encode key");
        (
            signing_key,
            BASE64_STANDARD.encode(private_key.as_bytes()),
        )
    }

    #[test]
    fn builds_agent_assertion_header_that_verifies() {
        let (signing_key, private_key) = test_private_key();
        let header = authorization_header_for_agent_task(
            "agent-runtime-1",
            &private_key,
            "task-run-1",
        )
        .expect("header");
        let token = header
            .strip_prefix("AgentAssertion ")
            .expect("scheme");
        let payload = URL_SAFE_NO_PAD.decode(token).expect("decode token");
        let envelope: Value = serde_json::from_slice(&payload).expect("json");

        assert_eq!(
            envelope.get("agent_runtime_id").and_then(Value::as_str),
            Some("agent-runtime-1")
        );
        assert_eq!(envelope.get("task_id").and_then(Value::as_str), Some("task-run-1"));
        let timestamp = envelope
            .get("timestamp")
            .and_then(Value::as_str)
            .expect("timestamp");
        let signature = BASE64_STANDARD
            .decode(envelope.get("signature").and_then(Value::as_str).expect("signature"))
            .expect("signature base64");
        let signature = Signature::from_slice(&signature).expect("signature bytes");
        signing_key
            .verifying_key()
            .verify(
                format!("agent-runtime-1:task-run-1:{timestamp}").as_bytes(),
                &signature,
            )
            .expect("signature verifies");
    }

    #[test]
    fn resolves_sub2api_agent_identity_fields() {
        let (_, private_key) = test_private_key();
        let secret = json!({
            "agent_runtime_id": "agent-runtime-1",
            "agent_private_key": private_key,
            "task_id": "task-run-1",
            "account_id": "account-1"
        });
        let config = json!({
            "auth_mode": "agentIdentity",
            "chatgpt_account_is_fedramp": true
        });

        let headers = resolve_agent_identity_headers(&secret, &config)
            .expect("ok")
            .expect("agent identity");

        assert!(headers.authorization.starts_with("AgentAssertion "));
        assert_eq!(headers.chatgpt_account_id, "account-1");
        assert!(headers.is_fedramp_account);
    }
}
