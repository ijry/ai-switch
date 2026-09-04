use crate::models::platform::{ApiDialect, PlatformId};
use crate::models::route_credential::RouteCredential;
use crate::models::route_credential_transfer::{
    RouteCredentialTransferIssue, TRANSFER_FORMAT, TRANSFER_SCHEMA_VERSION,
};
use crate::models::route_relay_balance::{
    RELAY_BALANCE_ACCESS_TOKEN_KEY, RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY,
};
use crate::services::official_agent_identity_service::{
    is_current_official_agent_identity_credential, validate_agent_identity_credential_fields,
};
use serde_json::{json, Map, Value};
use url::Url;

const OFFICIAL_SECRET_FIELDS: &[&str] = &[
    "id_token",
    "access_token",
    "refresh_token",
    "account_id",
    "workspace_id",
    "chatgpt_account_id",
    "agent_runtime_id",
    "agent_private_key",
    "task_id",
    "auth_mode",
    "chatgpt_account_is_fedramp",
    "client_id",
];

const OFFICIAL_CONFIG_FIELDS: &[&str] = &[
    "last_refresh",
    "expired",
    "expires_in",
    "disabled",
    "base_url",
    "token_endpoint",
    "auth_kind",
    "sub",
    "token_type",
    "redirect_uri",
    "headers",
];

const API_CONFIG_FIELDS: &[&str] = &[
    "base_url",
    "interface_format",
    "model_mappings",
    "responses_custom_tool_compat",
    "api_key_field",
    "headers",
    "turn_reminder",
    "turn_reminder_text",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedCredential {
    pub payload: Value,
    pub cpa_section: Option<String>,
    pub origin_format: String,
    pub warnings: Vec<RouteCredentialTransferIssue>,
}

pub fn project_credential(
    credential: &RouteCredential,
    instance_id: &str,
    in_pool: bool,
    include_enhanced_metadata: bool,
) -> Result<ProjectedCredential, RouteCredentialTransferIssue> {
    let platform = PlatformId::parse(&credential.platform)
        .map_err(|_| issue(credential, "transfer.platform_unknown", Some("platform")))?;
    let secret = parse_object(
        credential,
        &credential.secret_payload_json,
        "transfer.secret_json_invalid",
        "secret_payload_json",
    )?;
    let config = parse_object(
        credential,
        &credential.config_json,
        "transfer.config_json_invalid",
        "config_json",
    )?;

    match credential.kind.trim().to_ascii_lowercase().as_str() {
        "official" => project_official(
            credential,
            platform,
            &secret,
            &config,
            instance_id,
            in_pool,
            include_enhanced_metadata,
        ),
        "api" => project_api(
            credential,
            platform,
            &secret,
            &config,
            instance_id,
            in_pool,
            include_enhanced_metadata,
        ),
        _ => Err(issue(
            credential,
            "transfer.credential_kind_unsupported",
            Some("kind"),
        )),
    }
}

pub fn classify_api_section(
    platform: PlatformId,
    dialect: ApiDialect,
    base_url: &str,
) -> Result<&'static str, RouteCredentialTransferIssue> {
    match dialect {
        ApiDialect::Anthropic => Ok("claude-api-key"),
        ApiDialect::Gemini => Ok("gemini-api-key"),
        ApiDialect::OpenAiResponses => Ok("codex-api-key"),
        ApiDialect::OpenAi
            if platform == PlatformId::Grok && is_official_xai_endpoint(base_url) =>
        {
            Ok("xai-api-key")
        }
        ApiDialect::OpenAi => Ok("openai-compatibility"),
    }
}

pub fn trusted_cpa_raw_template(platform: &str, config: &Value) -> bool {
    let Some(config) = config.as_object() else {
        return false;
    };
    let Some(raw) = config.get("raw").and_then(Value::as_object) else {
        return false;
    };
    let Some(raw_type) = config.get("raw_type").and_then(nonempty_string) else {
        return false;
    };
    let Ok(expected_platform) = PlatformId::parse(platform) else {
        return false;
    };
    let Ok(raw_platform) = PlatformId::parse(raw_type) else {
        return false;
    };
    if expected_platform != raw_platform {
        return false;
    }

    for key in ["import_format", "source", "source_format", "origin_format"] {
        if let Some(marker) = config.get(key) {
            let Some(marker) = nonempty_string(marker) else {
                return false;
            };
            if !matches!(normalize_marker(marker).as_str(), "cpa" | "auth_file") {
                return false;
            }
        }
    }

    if let Some(provider) = config.get("provider") {
        let Some(provider) = nonempty_string(provider) else {
            return false;
        };
        if !PlatformId::parse(provider).is_ok_and(|provider| provider == expected_platform) {
            return false;
        }
    }

    for key in ["provider", "platform", "app"] {
        let Some(provider) = raw.get(key).and_then(nonempty_string) else {
            continue;
        };
        if !PlatformId::parse(provider).is_ok_and(|provider| provider == expected_platform) {
            return false;
        }
    }

    true
}

fn project_official(
    credential: &RouteCredential,
    platform: PlatformId,
    secret: &Map<String, Value>,
    config: &Map<String, Value>,
    instance_id: &str,
    in_pool: bool,
    include_enhanced_metadata: bool,
) -> Result<ProjectedCredential, RouteCredentialTransferIssue> {
    reject_unknown_secret_fields(credential, secret, OFFICIAL_SECRET_FIELDS)?;
    let config_value = Value::Object(config.clone());
    let trusted_raw = trusted_cpa_raw_template(platform.as_str(), &config_value);
    let mut warnings = Vec::new();
    let mut payload = if trusted_raw {
        config
            .get("raw")
            .and_then(Value::as_object)
            .map(flatten_auth_file_template)
            .unwrap_or_default()
    } else {
        if config
            .get("raw")
            .and_then(Value::as_object)
            .is_some_and(|raw| raw.values().any(is_nonempty))
        {
            warnings.push(issue(
                credential,
                "transfer.untrusted_raw_discarded",
                Some("raw"),
            ));
        }
        Map::new()
    };

    for field in OFFICIAL_SECRET_FIELDS {
        if *field == "client_id" {
            continue;
        }
        replace_current_field(&mut payload, secret, field);
    }
    for field in OFFICIAL_CONFIG_FIELDS {
        replace_current_field(&mut payload, config, field);
    }
    for field in ["client_id", "auth_mode", "chatgpt_account_is_fedramp"] {
        replace_preferred_current_field(&mut payload, secret, config, field);
    }

    remove_aliases(&mut payload, "email");
    if let Some(email) = credential
        .email
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        payload.insert("email".to_string(), json!(email));
    }
    payload.insert(
        "type".to_string(),
        json!(if platform == PlatformId::Grok {
            "xai"
        } else {
            platform.as_str()
        }),
    );
    for key in [
        "credentials",
        "tokens",
        "accounts",
        "provider",
        "platform",
        "app",
        "raw",
        "raw_type",
        "import_format",
        "preview",
        "preview_json",
    ] {
        payload.remove(key);
    }

    let secret_value = Value::Object(secret.clone());
    if is_current_official_agent_identity_credential(&secret_value, &config_value) {
        validate_agent_identity_credential_fields(&secret_value, &config_value).map_err(
            |field| {
                issue(
                    credential,
                    "transfer.agent_identity_field_required",
                    Some(field),
                )
            },
        )?;
    } else if !has_nonempty_field(secret, "access_token")
        && !has_nonempty_field(secret, "refresh_token")
    {
        return Err(issue(
            credential,
            "transfer.oauth_token_required",
            Some("access_token"),
        ));
    }

    let origin_format = detect_origin_format(config, trusted_raw);
    payload.insert(
        "x-ai-switch".to_string(),
        Value::Object(metadata(
            credential,
            instance_id,
            in_pool,
            include_enhanced_metadata,
            "official",
            None,
            &origin_format,
            config,
        )),
    );

    Ok(ProjectedCredential {
        payload: Value::Object(payload),
        cpa_section: None,
        origin_format,
        warnings,
    })
}

fn project_api(
    credential: &RouteCredential,
    platform: PlatformId,
    secret: &Map<String, Value>,
    config: &Map<String, Value>,
    instance_id: &str,
    in_pool: bool,
    include_enhanced_metadata: bool,
) -> Result<ProjectedCredential, RouteCredentialTransferIssue> {
    // The relay panel's account credentials are ours alone — the CPA format has no
    // slot for them, so they are tolerated and dropped rather than making the whole
    // export fail on an account that has them.
    reject_unknown_secret_fields(
        credential,
        secret,
        &[
            "api_key",
            RELAY_BALANCE_ACCESS_TOKEN_KEY,
            RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY,
        ],
    )?;
    let api_key = secret
        .get("api_key")
        .and_then(nonempty_string)
        .ok_or_else(|| issue(credential, "transfer.api_key_required", Some("api_key")))?;
    let base_url = config
        .get("base_url")
        .and_then(nonempty_string)
        .ok_or_else(|| issue(credential, "transfer.base_url_required", Some("base_url")))?;
    let interface_format = config
        .get("interface_format")
        .and_then(nonempty_string)
        .ok_or_else(|| {
            issue(
                credential,
                "transfer.interface_format_required",
                Some("interface_format"),
            )
        })?;
    let dialect = ApiDialect::parse(interface_format).map_err(|_| {
        issue(
            credential,
            "transfer.interface_format_unsupported",
            Some("interface_format"),
        )
    })?;
    let section = classify_api_section(platform, dialect, base_url)?;
    let models = project_models(config.get("model_mappings"));
    let headers = config
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut warnings = config
        .iter()
        .filter(|(field, value)| !API_CONFIG_FIELDS.contains(&field.as_str()) && is_nonempty(value))
        .map(|(field, _)| issue(credential, "transfer.api_config_field_ignored", Some(field)))
        .collect::<Vec<_>>();

    // The `relay_balance` config block already warns through the loop above, so the
    // panel credentials it needs are said out loud too. Otherwise the pair leaves
    // silently and the balance badge on the importing machine just stops working
    // with nothing to explain it.
    for field in [
        RELAY_BALANCE_ACCESS_TOKEN_KEY,
        RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY,
    ] {
        if secret.get(field).is_some_and(is_nonempty) {
            warnings.push(issue(
                credential,
                "transfer.relay_balance_secret_dropped",
                Some(field),
            ));
        }
    }

    if !include_enhanced_metadata {
        for field in [
            "responses_custom_tool_compat",
            "api_key_field",
            "turn_reminder",
            "turn_reminder_text",
        ] {
            if config.get(field).is_some_and(is_nonempty) {
                warnings.push(issue(
                    credential,
                    "transfer.enhanced_metadata_omitted",
                    Some(field),
                ));
            }
        }
    }

    let mut payload = Map::new();
    if section == "openai-compatibility" {
        payload.insert("name".to_string(), json!(credential.display_name));
        payload.insert("base-url".to_string(), json!(base_url));
        payload.insert("headers".to_string(), Value::Object(headers));
        payload.insert(
            "api-key-entries".to_string(),
            json!([{ "api-key": api_key }]),
        );
    } else {
        payload.insert("api-key".to_string(), json!(api_key));
        payload.insert("base-url".to_string(), json!(base_url));
        if !headers.is_empty() {
            payload.insert("headers".to_string(), Value::Object(headers));
        }
    }
    if !models.is_empty() {
        payload.insert("models".to_string(), Value::Array(models));
    }

    let origin_format = detect_origin_format(config, false);
    payload.insert(
        "x-ai-switch".to_string(),
        Value::Object(metadata(
            credential,
            instance_id,
            in_pool,
            include_enhanced_metadata,
            "api",
            Some(section),
            &origin_format,
            config,
        )),
    );

    Ok(ProjectedCredential {
        payload: Value::Object(payload),
        cpa_section: Some(section.to_string()),
        origin_format,
        warnings,
    })
}

fn metadata(
    credential: &RouteCredential,
    instance_id: &str,
    in_pool: bool,
    include_enhanced_metadata: bool,
    kind: &str,
    cpa_section: Option<&str>,
    origin_format: &str,
    config: &Map<String, Value>,
) -> Map<String, Value> {
    let mut metadata = Map::from_iter([
        ("format".to_string(), json!(TRANSFER_FORMAT)),
        ("schema_version".to_string(), json!(TRANSFER_SCHEMA_VERSION)),
        ("source_instance_id".to_string(), json!(instance_id)),
        ("source_credential_id".to_string(), json!(credential.id)),
        ("platform".to_string(), json!(credential.platform)),
        ("kind".to_string(), json!(kind)),
    ]);
    if let Some(cpa_section) = cpa_section {
        metadata.insert("cpa_section".to_string(), json!(cpa_section));
    }
    if !include_enhanced_metadata {
        return metadata;
    }

    metadata.insert("display_name".to_string(), json!(credential.display_name));
    metadata.insert("in_pool".to_string(), json!(in_pool));
    metadata.insert("origin_format".to_string(), json!(origin_format));
    if let Some(batch_id) = credential.batch_id.as_deref() {
        metadata.insert("source_batch_id".to_string(), json!(batch_id));
    }
    if let Some(batch_name) = credential.batch_name.as_deref() {
        metadata.insert("batch_name".to_string(), json!(batch_name));
    }
    if kind == "api" {
        for field in [
            "interface_format",
            "responses_custom_tool_compat",
            "api_key_field",
            "model_mappings",
            "turn_reminder",
            "turn_reminder_text",
        ] {
            if let Some(value) = config.get(field).filter(|value| is_nonempty(value)) {
                metadata.insert(field.to_string(), value.clone());
            }
        }
    }
    metadata
}

fn flatten_auth_file_template(raw: &Map<String, Value>) -> Map<String, Value> {
    let mut flattened = raw.clone();
    for nested_key in ["credentials", "tokens"] {
        if let Some(nested) = raw.get(nested_key).and_then(Value::as_object) {
            for (key, value) in nested {
                flattened
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            if let Some(tokens) = nested.get("tokens").and_then(Value::as_object) {
                for (key, value) in tokens {
                    flattened
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }
    flattened.remove("credentials");
    flattened.remove("tokens");
    flattened
}

fn reject_unknown_secret_fields(
    credential: &RouteCredential,
    secret: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), RouteCredentialTransferIssue> {
    if let Some((field, _)) = secret.iter().find(|(field, value)| {
        !allowed
            .iter()
            .any(|allowed_field| aliases(allowed_field).contains(&field.as_str()))
            && is_nonempty(value)
    }) {
        return Err(issue(
            credential,
            "transfer.secret_field_unsupported",
            Some(field),
        ));
    }
    Ok(())
}

fn replace_current_field(
    payload: &mut Map<String, Value>,
    source: &Map<String, Value>,
    field: &str,
) {
    remove_aliases(payload, field);
    let value = aliases(field)
        .iter()
        .find_map(|alias| source.get(*alias))
        .filter(|value| is_nonempty(value));
    if let Some(value) = value {
        if field == "headers" && !value.is_object() {
            return;
        }
        payload.insert(field.to_string(), value.clone());
    }
}

fn replace_preferred_current_field(
    payload: &mut Map<String, Value>,
    secret: &Map<String, Value>,
    config: &Map<String, Value>,
    field: &str,
) {
    remove_aliases(payload, field);
    let source = if aliases(field)
        .iter()
        .any(|alias| secret.contains_key(*alias))
    {
        Some(secret)
    } else if aliases(field)
        .iter()
        .any(|alias| config.contains_key(*alias))
    {
        Some(config)
    } else {
        None
    };
    let value = source
        .and_then(|source| aliases(field).iter().find_map(|alias| source.get(*alias)))
        .filter(|value| is_nonempty(value));
    if let Some(value) = value {
        payload.insert(field.to_string(), value.clone());
    }
}

fn remove_aliases(payload: &mut Map<String, Value>, field: &str) {
    for alias in aliases(field) {
        payload.remove(*alias);
    }
}

fn has_nonempty_field(object: &Map<String, Value>, field: &str) -> bool {
    aliases(field)
        .iter()
        .find_map(|alias| object.get(*alias))
        .is_some_and(is_nonempty)
}

fn aliases(field: &str) -> &'static [&'static str] {
    match field {
        "id_token" => &["id_token", "idToken"],
        "access_token" => &["access_token", "accessToken"],
        "refresh_token" => &["refresh_token", "refreshToken"],
        "account_id" => &["account_id", "accountId"],
        "workspace_id" => &["workspace_id", "workspaceId"],
        "chatgpt_account_id" => &["chatgpt_account_id", "chatgptAccountId"],
        "agent_runtime_id" => &["agent_runtime_id", "agentRuntimeId"],
        "agent_private_key" => &["agent_private_key", "agentPrivateKey"],
        "task_id" => &["task_id", "taskId"],
        "auth_mode" => &["auth_mode", "authMode"],
        "chatgpt_account_is_fedramp" => &[
            "chatgpt_account_is_fedramp",
            "chatgptAccountIsFedramp",
            "is_fedramp_account",
            "isFedrampAccount",
        ],
        "client_id" => &["client_id", "clientId"],
        "last_refresh" => &["last_refresh", "lastRefresh"],
        "expires_in" => &["expires_in", "expiresIn"],
        "base_url" => &["base_url", "baseUrl"],
        "token_endpoint" => &["token_endpoint", "tokenEndpoint"],
        "auth_kind" => &["auth_kind", "authKind"],
        "token_type" => &["token_type", "tokenType"],
        "redirect_uri" => &["redirect_uri", "redirectUri"],
        "email" => &["email"],
        "expired" => &["expired"],
        "disabled" => &["disabled"],
        "sub" => &["sub"],
        "headers" => &["headers"],
        "api_key" => &["api_key"],
        RELAY_BALANCE_ACCESS_TOKEN_KEY => &[RELAY_BALANCE_ACCESS_TOKEN_KEY],
        RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY => &[RELAY_BALANCE_ACCESS_TOKEN_USER_ID_KEY],
        _ => &[],
    }
}

fn project_models(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|mapping| {
            let mapping = mapping.as_object()?;
            let mut projected = Map::new();
            for (source, target) in [("to", "name"), ("from", "alias"), ("label", "display-name")] {
                if let Some(value) = mapping.get(source).and_then(nonempty_string) {
                    projected.insert(target.to_string(), json!(value));
                }
            }
            if mapping.get("supports_1m").and_then(Value::as_bool) == Some(true) {
                projected.insert("max-context-length".to_string(), json!(1_048_576));
            }
            (!projected.is_empty()).then_some(Value::Object(projected))
        })
        .collect()
}

fn detect_origin_format(config: &Map<String, Value>, trusted_raw: bool) -> String {
    for key in ["import_format", "origin_format"] {
        let Some(value) = config.get(key).and_then(nonempty_string) else {
            continue;
        };
        let normalized = normalize_marker(value);
        if matches!(
            normalized.as_str(),
            "cpa" | "auth_file" | "sub2api" | "ai_switch"
        ) {
            return normalized.replace('_', "-");
        }
    }
    if trusted_raw {
        "cpa".to_string()
    } else {
        "ai-switch".to_string()
    }
}

fn parse_object(
    credential: &RouteCredential,
    text: &str,
    code: &'static str,
    field: &'static str,
) -> Result<Map<String, Value>, RouteCredentialTransferIssue> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| issue(credential, code, Some(field)))
}

fn issue(
    credential: &RouteCredential,
    code: &str,
    field: Option<&str>,
) -> RouteCredentialTransferIssue {
    RouteCredentialTransferIssue {
        item_index: None,
        display_name: Some(credential.display_name.clone()),
        code: code.to_string(),
        field: field.map(str::to_string),
    }
}

fn nonempty_string(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

fn normalize_marker(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn is_official_xai_endpoint(base_url: &str) -> bool {
    Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "api.x.ai")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(kind: &str, platform: &str, secret: Value, config: Value) -> RouteCredential {
        RouteCredential {
            id: "credential-1".to_string(),
            platform: platform.to_string(),
            kind: kind.to_string(),
            display_name: "Test credential".to_string(),
            email: Some("current@example.com".to_string()),
            status: "ok".to_string(),
            sort_order: 0,
            route_priority: 3,
            max_concurrency: 1,
            batch_id: Some("batch-1".to_string()),
            batch_name: Some("Batch One".to_string()),
            secret_payload_json: secret.to_string(),
            config_json: config.to_string(),
            preview_json: "{}".to_string(),
            subscription_type: None,
            primary_remain: None,
            weekly_remain: None,
            reset_primary: None,
            reset_weekly: None,
            transient_failure_count: 0,
            next_retry_at: None,
            cooldown_until: None,
            last_failure_kind: None,
            last_failure_message: None,
            last_failure_response_json: None,
            active_request_count: 0,
            model_states: Vec::new(),
            request_count: 0,
            success_count: 0,
            failure_count: 0,
            success_rate: None,
            quota_remaining: None,
            quota_limit: None,
            quota_used: None,
            quota_updated_at: None,
            archived_at: None,
            created_at: "2026-08-04T00:00:00Z".to_string(),
            updated_at: "2026-08-04T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn official_projection_reconciles_trusted_raw_with_current_fields() {
        let credential = credential(
            "official",
            "codex",
            json!({
                "access_token": "current-access",
                "refresh_token": "",
                "client_id": "current-client"
            }),
            json!({
                "raw_type": "codex",
                "raw": {
                    "type": "codex",
                    "accessToken": "stale-access",
                    "refresh_token": "stale-refresh",
                    "clientId": "stale-client",
                    "future-cpa-field": "preserved"
                }
            }),
        );

        let projected = project_credential(&credential, "instance-1", true, true).unwrap();
        let payload = projected.payload.as_object().unwrap();

        assert_eq!(payload["access_token"], "current-access");
        assert_eq!(payload["client_id"], "current-client");
        assert_eq!(payload["future-cpa-field"], "preserved");
        assert!(!payload.contains_key("accessToken"));
        assert!(!payload.contains_key("refresh_token"));
        assert!(!payload.contains_key("clientId"));
    }

    #[test]
    fn official_projection_removes_currently_empty_client_id() {
        let credential = credential(
            "official",
            "claude",
            json!({"access_token": "current", "client_id": ""}),
            json!({
                "raw_type": "anthropic",
                "raw": {"type": "claude", "client_id": "stale", "clientId": "stale-2"}
            }),
        );

        let projected = project_credential(&credential, "instance-1", false, true).unwrap();
        let payload = projected.payload.as_object().unwrap();
        assert!(!payload.contains_key("client_id"));
        assert!(!payload.contains_key("clientId"));
    }

    #[test]
    fn official_projection_removes_stale_client_id_when_current_field_is_absent() {
        let credential = credential(
            "official",
            "claude",
            json!({"access_token": "current"}),
            json!({
                "raw_type": "anthropic",
                "raw": {"type": "claude", "client_id": "stale", "clientId": "stale-2"}
            }),
        );

        let projected = project_credential(&credential, "instance-1", false, true).unwrap();
        let payload = projected.payload.as_object().unwrap();
        assert!(!payload.contains_key("client_id"));
        assert!(!payload.contains_key("clientId"));
    }

    #[test]
    fn official_projection_flattens_nested_fields_and_normalizes_grok_type() {
        let credential = credential(
            "official",
            "grok",
            json!({"access_token": "current", "account_id": "account-1"}),
            json!({
                "import_format": "sub2api",
                "raw_type": "oauth",
                "base_url": "https://api.x.ai",
                "raw": {
                    "provider": "xai",
                    "credentials": {"accessToken": "stale", "accountId": "stale-account"}
                }
            }),
        );

        let projected = project_credential(&credential, "instance-1", false, true).unwrap();
        let payload = projected.payload.as_object().unwrap();
        assert_eq!(payload["type"], "xai");
        assert_eq!(payload["access_token"], "current");
        assert_eq!(payload["account_id"], "account-1");
        assert!(!payload.contains_key("credentials"));
        assert!(projected
            .warnings
            .iter()
            .any(|issue| issue.code == "transfer.untrusted_raw_discarded"));
    }

    #[test]
    fn official_projection_requires_oauth_or_complete_agent_identity() {
        let oauth = credential("official", "codex", json!({"id_token": "id"}), json!({}));
        assert_eq!(
            project_credential(&oauth, "instance-1", false, true)
                .unwrap_err()
                .code,
            "transfer.oauth_token_required"
        );

        let agent = credential(
            "official",
            "codex",
            json!({
                "auth_mode": "agentIdentity",
                "agent_private_key": "private",
                "agent_runtime_id": "runtime",
                "account_id": "account"
            }),
            json!({}),
        );
        let error = project_credential(&agent, "instance-1", false, true).unwrap_err();
        assert_eq!(error.code, "transfer.agent_identity_field_required");
        assert_eq!(error.field.as_deref(), Some("task_id"));
    }

    #[test]
    fn agent_identity_validation_does_not_reuse_stale_raw_fields() {
        let credential = credential(
            "official",
            "codex",
            json!({
                "auth_mode": "agentIdentity",
                "agent_private_key": "",
                "agent_runtime_id": "runtime",
                "task_id": "task",
                "account_id": "account"
            }),
            json!({
                "raw_type": "codex",
                "raw": {
                    "type": "codex",
                    "agentPrivateKey": "stale-private-key"
                }
            }),
        );

        let error = project_credential(&credential, "instance-1", false, true).unwrap_err();
        assert_eq!(error.code, "transfer.agent_identity_field_required");
        assert_eq!(error.field.as_deref(), Some("agent_private_key"));
    }

    #[test]
    fn api_section_classification_matches_contract() {
        assert_eq!(
            classify_api_section(
                PlatformId::Claude,
                ApiDialect::Anthropic,
                "https://api.anthropic.com"
            )
            .unwrap(),
            "claude-api-key"
        );
        assert_eq!(
            classify_api_section(
                PlatformId::Gemini,
                ApiDialect::Gemini,
                "https://generativelanguage.googleapis.com"
            )
            .unwrap(),
            "gemini-api-key"
        );
        assert_eq!(
            classify_api_section(
                PlatformId::Codex,
                ApiDialect::OpenAiResponses,
                "https://api.openai.com/v1"
            )
            .unwrap(),
            "codex-api-key"
        );
        assert_eq!(
            classify_api_section(
                PlatformId::Grok,
                ApiDialect::OpenAi,
                "HTTPS://API.X.AI:443/v1/"
            )
            .unwrap(),
            "xai-api-key"
        );
        assert_eq!(
            classify_api_section(
                PlatformId::Codex,
                ApiDialect::OpenAi,
                "https://openrouter.ai/api/v1"
            )
            .unwrap(),
            "openai-compatibility"
        );
    }

    #[test]
    fn api_projection_uses_cpa_entry_shape_and_maps_models() {
        let credential = credential(
            "api",
            "codex",
            json!({"api_key": "sk-test"}),
            json!({
                "base_url": "https://openrouter.ai/api/v1",
                "interface_format": "openai",
                "headers": {"X-Test": "value"},
                "model_mappings": [{
                    "from": "gpt-5",
                    "to": "provider-gpt-5",
                    "label": "GPT 5",
                    "supports_1m": true
                }]
            }),
        );

        let projected = project_credential(&credential, "instance-1", true, true).unwrap();
        let payload = projected.payload.as_object().unwrap();
        assert!(!payload.contains_key("type"));
        assert_eq!(payload["api-key-entries"].as_array().unwrap().len(), 1);
        assert_eq!(payload["api-key-entries"][0]["api-key"], "sk-test");
        assert_eq!(payload["models"][0]["name"], "provider-gpt-5");
        assert_eq!(payload["models"][0]["alias"], "gpt-5");
        assert_eq!(payload["models"][0]["display-name"], "GPT 5");
        assert_eq!(payload["models"][0]["max-context-length"], 1_048_576);
    }

    /// The relay panel's account access token shares the secret payload with the
    /// api_key, and an unknown secret field fails the whole export. The CPA format
    /// has no slot for it, so it is tolerated and left behind rather than turning
    /// every account that reads an account-level balance into an export error.
    #[test]
    fn api_projection_tolerates_the_relay_panel_access_token() {
        let credential = credential(
            "api",
            "codex",
            json!({
                "api_key": "sk-test",
                "relay_balance_access_token": "pat-panel-token",
                "relay_balance_access_token_user_id": "7",
            }),
            json!({
                "base_url": "https://panel.example.com/v1",
                "interface_format": "openai",
                "model_mappings": [],
            }),
        );

        let projected = project_credential(&credential, "instance-1", true, true).unwrap();
        let payload = projected.payload.as_object().unwrap();
        assert_eq!(payload["api-key-entries"][0]["api-key"], "sk-test");
        for field in [
            "relay_balance_access_token",
            "relay_balance_access_token_user_id",
        ] {
            assert!(
                !payload.contains_key(field),
                "{field} is ours, not part of the interchange format"
            );
            // Leaving quietly would break the balance badge on the importing
            // machine with nothing on screen to explain it.
            assert!(
                projected.warnings.iter().any(|warning| {
                    warning.code == "transfer.relay_balance_secret_dropped"
                        && warning.field.as_deref() == Some(field)
                }),
                "{field} left without saying so: {:?}",
                projected.warnings
            );
        }
    }

    /// A secret field nobody knows still fails, so tolerating one key did not turn
    /// the check off.
    #[test]
    fn api_projection_still_rejects_an_unknown_secret_field() {
        let credential = credential(
            "api",
            "codex",
            json!({"api_key": "sk-test", "mystery": "value"}),
            json!({
                "base_url": "https://panel.example.com/v1",
                "interface_format": "openai",
                "model_mappings": [],
            }),
        );

        let issue = project_credential(&credential, "instance-1", true, true)
            .expect_err("an unknown secret field is not exportable");
        assert_eq!(issue.code, "transfer.secret_field_unsupported");
        assert_eq!(issue.field.as_deref(), Some("mystery"));
    }

    #[test]
    fn trusted_raw_predicate_accepts_only_cpa_auth_file_sources() {
        assert!(trusted_cpa_raw_template(
            "codex",
            &json!({"raw_type": "codex", "raw": {"type": "codex"}})
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({
                "import_format": "sub2api",
                "raw_type": "codex",
                "raw": {"type": "codex"}
            })
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({"raw_type": "arbitrary-provider", "raw": {"type": "codex"}})
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({"raw_type": "codex", "raw": "not-an-object"})
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({
                "source": "arbitrary-provider",
                "raw_type": "codex",
                "raw": {"type": "codex"}
            })
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({
                "origin_format": "ai-switch",
                "raw_type": "codex",
                "raw": {"type": "codex"}
            })
        ));
        assert!(!trusted_cpa_raw_template(
            "codex",
            &json!({
                "provider": "arbitrary-provider",
                "raw_type": "codex",
                "raw": {"type": "codex"}
            })
        ));
    }

    #[test]
    fn secret_field_errors_never_include_secret_values() {
        let synthetic_secret = "synthetic-secret-value";
        let credential = credential(
            "api",
            "codex",
            json!({"api_key": "sk-test", "unsafe_secret": synthetic_secret}),
            json!({"base_url": "https://api.example.com", "interface_format": "openai"}),
        );

        let error = project_credential(&credential, "instance-1", false, true).unwrap_err();
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains(synthetic_secret));
        assert_eq!(error.field.as_deref(), Some("unsafe_secret"));
    }
}
