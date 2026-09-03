use crate::error::{ApiError, AppError};
use crate::models::platform::{ApiDialect, PlatformId, PlatformOperation};
use crate::models::route_credential::{CreateApiRouteCredentialInput, ModelMapping};
use crate::services::platform_capability_service::PlatformCapabilityService;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeepLinkProviderImport {
    pub scheme: String,
    pub version: String,
    pub resource: String,
    pub app: String,
    pub platform: String,
    pub display_name: String,
    pub base_url: String,
    pub api_key_masked: String,
    pub api_key: String,
    pub interface_format: String,
    pub model_mappings_json: String,
    pub homepage: Option<String>,
    pub notes: Option<String>,
    pub source_url_sanitized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeepLinkErrorPayload {
    pub message: String,
    pub source: String,
}

pub struct DeepLinkBuildInput<'a> {
    pub platform: &'a str,
    pub display_name: &'a str,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub interface_format: &'a str,
    pub model_mappings: &'a [ModelMapping],
    pub headers: &'a serde_json::Value,
    pub api_key_field: Option<&'a str>,
    pub responses_custom_tool_compat: bool,
}

pub fn build_aiswitch_import_url(input: &DeepLinkBuildInput<'_>) -> Result<String, String> {
    validate_untrimmed_required(input.display_name, "deeplink_export.name_unsupported")?;
    validate_untrimmed_required(input.api_key, "deeplink_export.api_key_unsupported")?;
    validate_endpoint(input.base_url)?;

    if !matches!(input.headers, serde_json::Value::Null)
        && input
            .headers
            .as_object()
            .is_none_or(|headers| !headers.is_empty())
    {
        return Err("deeplink_export.headers_unsupported".into());
    }
    if input.api_key_field.is_some() {
        return Err("deeplink_export.api_key_field_unsupported".into());
    }
    if input.responses_custom_tool_compat {
        return Err("deeplink_export.custom_tool_compat_unsupported".into());
    }
    if input
        .model_mappings
        .iter()
        .any(|mapping| mapping.supports_1m.is_some())
    {
        return Err("deeplink_export.supports_1m_unsupported".into());
    }

    let platform = PlatformId::parse(input.platform)
        .map_err(|_| "deeplink_export.platform_unsupported".to_string())?;
    PlatformCapabilityService::require(platform, PlatformOperation::DeeplinkImport)
        .map_err(|_| "deeplink_export.platform_unsupported".to_string())?;
    let dialect = ApiDialect::parse(input.interface_format)
        .map_err(|_| "deeplink_export.dialect_unsupported".to_string())?;
    if dialect != default_deeplink_dialect(platform) {
        return Err("deeplink_export.dialect_unsupported".into());
    }

    let model_params = build_model_query_pairs(platform, input.model_mappings)?;
    let mut url = Url::parse("aiswitch://v1/import")
        .map_err(|_| "deeplink_export.url_construction_failed".to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("resource", "provider");
        query.append_pair("app", platform.as_str());
        query.append_pair("name", input.display_name);
        query.append_pair("endpoint", input.base_url);
        query.append_pair("apiKey", input.api_key);
        for (key, value) in model_params {
            query.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

pub fn parse_deeplink_url(url_str: &str) -> Result<DeepLinkProviderImport, String> {
    let url = Url::parse(url_str).map_err(|err| format!("无效的深链接 URL: {err}"))?;
    let scheme = url.scheme().to_string();
    if scheme != "ccswitch" && scheme != "aiswitch" {
        return Err(format!("不支持的 scheme: {scheme}"));
    }

    let version = url
        .host_str()
        .ok_or_else(|| "缺少协议版本".to_string())?
        .to_string();
    if version != "v1" {
        return Err(format!("不支持的协议版本: {version}"));
    }
    if url.path() != "/import" {
        return Err(format!("不支持的路径: {}", url.path()));
    }

    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let resource = required_param(&params, "resource")?;
    if resource != "provider" {
        return Err(format!("暂不支持的 resource: {resource}"));
    }

    let app = required_param(&params, "app")?;
    let display_name = required_param(&params, "name")?;
    let (platform, interface_format) = map_app(&app).map_err(format_deeplink_app_error)?;
    let base_url = first_valid_endpoint(params.get("endpoint").map(String::as_str))?;
    let api_key = required_param(&params, "apiKey")?;
    let model_mappings_json = build_model_mappings_json(&app, &platform, &params)?;
    let homepage = optional_nonempty(&params, "homepage");
    let notes = optional_nonempty(&params, "notes");

    Ok(DeepLinkProviderImport {
        scheme,
        version,
        resource,
        app,
        platform,
        display_name,
        base_url,
        api_key_masked: mask_api_key(&api_key),
        api_key,
        interface_format,
        model_mappings_json,
        homepage,
        notes,
        source_url_sanitized: sanitize_source_url(url_str),
    })
}

pub fn to_create_api_input(parsed: &DeepLinkProviderImport) -> CreateApiRouteCredentialInput {
    CreateApiRouteCredentialInput {
        platform: parsed.platform.clone(),
        display_name: parsed.display_name.clone(),
        api_key: parsed.api_key.clone(),
        base_url: parsed.base_url.clone(),
        interface_format: parsed.interface_format.clone(),
        model_mappings_json: parsed.model_mappings_json.clone(),
        fetched_models_json: None,
        api_key_field: None,
        preview_json: None,
        batch_id: None,
        responses_custom_tool_compat: None,
        user_agent: None,
        relay_balance_provider: None,
    }
}

pub fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    let characters: Vec<char> = trimmed.chars().collect();
    if characters.len() <= 8 {
        let prefix: String = characters.iter().take(2).collect();
        return format!("{prefix}***");
    }
    let prefix: String = characters.iter().take(4).collect();
    let suffix: String = characters[characters.len() - 4..].iter().collect();
    format!("{prefix}***{suffix}")
}

pub fn sanitize_source_url(url_str: &str) -> String {
    let Ok(mut url) = Url::parse(url_str) else {
        return "(invalid-url)".to_string();
    };

    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            if key.eq_ignore_ascii_case("apiKey") {
                (key.into_owned(), mask_api_key(&value))
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect();

    url.set_query(None);
    if !pairs.is_empty() {
        let mut serializer = url.query_pairs_mut();
        for (key, value) in pairs {
            serializer.append_pair(&key, &value);
        }
    }
    url.to_string()
}

fn map_app(app: &str) -> Result<(String, String), AppError> {
    let platform = PlatformId::parse(app)?;
    PlatformCapabilityService::require(platform, PlatformOperation::DeeplinkImport)?;
    let dialect = default_deeplink_dialect(platform);
    Ok((platform.as_str().to_string(), dialect.as_str().to_string()))
}

fn default_deeplink_dialect(platform: PlatformId) -> ApiDialect {
    match platform {
        PlatformId::Codex => ApiDialect::OpenAiResponses,
        PlatformId::Claude => ApiDialect::Anthropic,
        PlatformId::Gemini => ApiDialect::Gemini,
        PlatformId::Grok => ApiDialect::OpenAi,
        PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes => {
            unreachable!("capability guard rejects unsupported Deeplink platforms")
        }
    }
}

fn validate_untrimmed_required(value: &str, code: &'static str) -> Result<(), String> {
    if value.is_empty() || value.trim() != value {
        return Err(code.into());
    }
    Ok(())
}

fn validate_endpoint(base_url: &str) -> Result<(), String> {
    validate_untrimmed_required(base_url, "deeplink_export.endpoint_unsupported")?;
    if base_url.contains(',') {
        return Err("deeplink_export.endpoint_unsupported".into());
    }
    let endpoint =
        Url::parse(base_url).map_err(|_| "deeplink_export.endpoint_unsupported".to_string())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("deeplink_export.endpoint_unsupported".into());
    }
    Ok(())
}

fn build_model_query_pairs<'a>(
    platform: PlatformId,
    mappings: &'a [ModelMapping],
) -> Result<Vec<(&'static str, &'a str)>, String> {
    if platform == PlatformId::Claude {
        return build_claude_model_query_pairs(mappings);
    }
    if mappings.len() > 1 {
        return Err("deeplink_export.multiple_models_unsupported".into());
    }
    let Some(mapping) = mappings.first() else {
        return Ok(Vec::new());
    };
    let expected_from = match platform {
        PlatformId::Codex => "gpt-5",
        PlatformId::Gemini => "gemini-2.5-flash",
        PlatformId::Grok => "grok-3",
        PlatformId::Claude | PlatformId::OpenCode | PlatformId::OpenClaw | PlatformId::Hermes => {
            unreachable!("handled or rejected before model serialization")
        }
    };
    if mapping.from != expected_from
        || mapping.to.is_empty()
        || mapping.to.trim() != mapping.to
        || mapping.label.is_some()
    {
        return Err("deeplink_export.models_unsupported".into());
    }
    Ok(vec![("model", mapping.to.as_str())])
}

fn build_claude_model_query_pairs<'a>(
    mappings: &'a [ModelMapping],
) -> Result<Vec<(&'static str, &'a str)>, String> {
    if mappings.len() > 3 {
        return Err("deeplink_export.claude_models_unsupported".into());
    }

    let roles = [
        ("claude-haiku-alias", "Haiku", "haikuModel"),
        ("claude-sonnet-alias", "Sonnet", "sonnetModel"),
        ("claude-opus-alias", "Opus", "opusModel"),
    ];
    let mut result = Vec::with_capacity(mappings.len());
    let mut previous_role = None;
    for mapping in mappings {
        let Some((role_index, (_, expected_label, query_key))) = roles
            .iter()
            .enumerate()
            .find(|(_, (from, _, _))| mapping.from == *from)
        else {
            return Err("deeplink_export.claude_models_unsupported".into());
        };
        if previous_role.is_some_and(|previous| role_index <= previous)
            || mapping.label.as_deref() != Some(*expected_label)
            || mapping.to.is_empty()
            || mapping.to.trim() != mapping.to
        {
            return Err("deeplink_export.claude_models_unsupported".into());
        }
        previous_role = Some(role_index);
        result.push((*query_key, mapping.to.as_str()));
    }
    Ok(result)
}

fn format_deeplink_app_error(error: AppError) -> String {
    let error = ApiError::from(error);
    format!("{}: {}", error.code, error.message)
}

fn first_valid_endpoint(raw: Option<&str>) -> Result<String, String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("缺少 endpoint".into());
    };

    for part in raw.split(',') {
        let candidate = part.trim();
        if candidate.is_empty() {
            continue;
        }
        if let Ok(parsed) = Url::parse(candidate) {
            if parsed.scheme() == "http" || parsed.scheme() == "https" {
                return Ok(candidate.to_string());
            }
        }
    }

    Err("没有有效的 http(s) endpoint".into())
}

fn build_model_mappings_json(
    app: &str,
    platform: &str,
    params: &HashMap<String, String>,
) -> Result<String, String> {
    let mut mappings = Vec::new();

    if platform == "claude" {
        push_claude_mapping(
            &mut mappings,
            params,
            "haikuModel",
            "claude-haiku-alias",
            "Haiku",
        );
        push_claude_mapping(
            &mut mappings,
            params,
            "sonnetModel",
            "claude-sonnet-alias",
            "Sonnet",
        );
        push_claude_mapping(
            &mut mappings,
            params,
            "opusModel",
            "claude-opus-alias",
            "Opus",
        );
    } else if let Some(model) = optional_nonempty(params, "model") {
        let from = match platform {
            "codex" => "gpt-5",
            "gemini" => "gemini-2.5-flash",
            "grok" => "grok-3",
            _ => return Err(format!("无法为应用 {app} 生成模型映射")),
        };
        mappings.push(ModelMapping {
            from: from.into(),
            to: model,
            label: None,
            supports_1m: None,
            ..Default::default()
        });
    }

    serde_json::to_string(&mappings).map_err(|err| format!("模型映射序列化失败: {err}"))
}

fn push_claude_mapping(
    out: &mut Vec<ModelMapping>,
    params: &HashMap<String, String>,
    key: &str,
    from: &str,
    label: &str,
) {
    if let Some(to) = optional_nonempty(params, key) {
        out.push(ModelMapping {
            from: from.into(),
            to,
            label: Some(label.into()),
            supports_1m: None,
            ..Default::default()
        });
    }
}

fn required_param(params: &HashMap<String, String>, key: &str) -> Result<String, String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("缺少参数: {key}"))
}

fn optional_nonempty(params: &HashMap<String, String>, key: &str) -> Option<String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn build_input<'a>(
        platform: &'a str,
        interface_format: &'a str,
        model_mappings: &'a [ModelMapping],
        headers: &'a serde_json::Value,
    ) -> DeepLinkBuildInput<'a> {
        DeepLinkBuildInput {
            platform,
            display_name: "Route & Name",
            base_url: "https://api.example.com/v1?region=us&mode=fast",
            api_key: "sk-secret-key-value",
            interface_format,
            model_mappings,
            headers,
            api_key_field: None,
            responses_custom_tool_compat: false,
        }
    }

    fn assert_round_trip(
        platform: &str,
        interface_format: &str,
        mappings: &[ModelMapping],
        expected_mappings: &[ModelMapping],
    ) {
        let headers = json!({});
        let input = build_input(platform, interface_format, mappings, &headers);
        let link = build_aiswitch_import_url(&input).expect("build");
        assert!(link.starts_with("aiswitch://v1/import?resource=provider"));

        let parsed = parse_deeplink_url(&link).expect("parse built link");
        assert_eq!(parsed.platform, platform);
        assert_eq!(parsed.display_name, input.display_name);
        assert_eq!(parsed.base_url, input.base_url);
        assert_eq!(parsed.api_key, input.api_key);
        assert_eq!(parsed.interface_format, interface_format);
        let parsed_mappings: Vec<ModelMapping> =
            serde_json::from_str(&parsed.model_mappings_json).expect("model mappings");
        assert_eq!(parsed_mappings, expected_mappings);
    }

    fn assert_safe_build_error(input: &DeepLinkBuildInput<'_>, expected: &str) {
        let error = match build_aiswitch_import_url(input) {
            Err(error) => error,
            Ok(_) => panic!("expected safe build rejection"),
        };
        assert_eq!(error, expected);
        assert!(!error.contains(input.api_key));
        assert!(!error.contains(input.base_url));
        assert!(!error.contains("aiswitch://"));
    }

    #[test]
    fn parses_and_masks_unicode_api_key_without_panicking() {
        let headers = json!({});
        let mappings = Vec::new();
        let mut input = build_input("codex", "openai-responses", &mappings, &headers);
        input.api_key = "密钥🔑秘密";
        let url = build_aiswitch_import_url(&input).expect("build Unicode API key");

        let parsed = parse_deeplink_url(&url).expect("parse Unicode API key");
        assert_eq!(parsed.api_key.chars().count(), 5);
        assert!(!parsed.api_key_masked.contains("密钥🔑秘密"));
        assert!(!parsed.source_url_sanitized.contains("密钥🔑秘密"));
    }

    #[test]
    fn builds_supported_aiswitch_links_losslessly() {
        let codex_mappings = vec![ModelMapping {
            from: "gpt-5".into(),
            to: "gpt-5.2-codex".into(),
            label: None,
            supports_1m: None,
            ..Default::default()
        }];
        assert_round_trip(
            "codex",
            "openai-responses",
            &codex_mappings,
            &codex_mappings,
        );

        let claude_mappings = vec![
            ModelMapping {
                from: "claude-haiku-alias".into(),
                to: "claude-haiku-custom".into(),
                label: Some("Haiku".into()),
                supports_1m: None,
                ..Default::default()
            },
            ModelMapping {
                from: "claude-sonnet-alias".into(),
                to: "claude-sonnet-custom".into(),
                label: Some("Sonnet".into()),
                supports_1m: None,
                ..Default::default()
            },
            ModelMapping {
                from: "claude-opus-alias".into(),
                to: "claude-opus-custom".into(),
                label: Some("Opus".into()),
                supports_1m: None,
                ..Default::default()
            },
        ];
        assert_round_trip("claude", "anthropic", &claude_mappings, &claude_mappings);

        let gemini_mappings = vec![ModelMapping {
            from: "gemini-2.5-flash".into(),
            to: "gemini-3-pro".into(),
            label: None,
            supports_1m: None,
            ..Default::default()
        }];
        assert_round_trip("gemini", "gemini", &gemini_mappings, &gemini_mappings);

        let grok_mappings = vec![ModelMapping {
            from: "grok-3".into(),
            to: "grok-4.5".into(),
            label: None,
            supports_1m: None,
            ..Default::default()
        }];
        assert_round_trip("grok", "openai", &grok_mappings, &grok_mappings);
    }

    #[test]
    fn rejects_lossy_aiswitch_link_exports_with_safe_codes() {
        let empty_headers = json!({});
        let custom_headers = json!({"X-Secret": "header-secret"});
        let empty_mappings = Vec::new();

        let input = build_input(
            "codex",
            "openai-responses",
            &empty_mappings,
            &custom_headers,
        );
        assert_safe_build_error(&input, "deeplink_export.headers_unsupported");

        let mut input = build_input("claude", "anthropic", &empty_mappings, &empty_headers);
        input.api_key_field = Some("ANTHROPIC_AUTH_TOKEN");
        assert_safe_build_error(&input, "deeplink_export.api_key_field_unsupported");

        let mut input = build_input("codex", "openai-responses", &empty_mappings, &empty_headers);
        input.responses_custom_tool_compat = true;
        assert_safe_build_error(&input, "deeplink_export.custom_tool_compat_unsupported");

        let multiple_mappings = vec![
            ModelMapping {
                from: "gpt-5".into(),
                to: "gpt-5.1".into(),
                label: None,
                supports_1m: None,
                ..Default::default()
            },
            ModelMapping {
                from: "gpt-5".into(),
                to: "gpt-5.2".into(),
                label: None,
                supports_1m: None,
                ..Default::default()
            },
        ];
        let input = build_input(
            "codex",
            "openai-responses",
            &multiple_mappings,
            &empty_headers,
        );
        assert_safe_build_error(&input, "deeplink_export.multiple_models_unsupported");

        let supports_1m = vec![ModelMapping {
            from: "claude-sonnet-alias".into(),
            to: "claude-sonnet-custom".into(),
            label: Some("Sonnet".into()),
            supports_1m: Some(true),
            ..Default::default()
        }];
        let input = build_input("claude", "anthropic", &supports_1m, &empty_headers);
        assert_safe_build_error(&input, "deeplink_export.supports_1m_unsupported");

        let input = build_input("opencode", "openai", &empty_mappings, &empty_headers);
        assert_safe_build_error(&input, "deeplink_export.platform_unsupported");

        let input = build_input("codex", "openai", &empty_mappings, &empty_headers);
        assert_safe_build_error(&input, "deeplink_export.dialect_unsupported");

        let mut input = build_input("codex", "openai-responses", &empty_mappings, &empty_headers);
        input.base_url = "file:///tmp/secret-route.json";
        assert_safe_build_error(&input, "deeplink_export.endpoint_unsupported");

        let mut input = build_input("codex", "openai-responses", &empty_mappings, &empty_headers);
        input.base_url = "https://api.example.com/models,audit?region=us";
        assert_safe_build_error(&input, "deeplink_export.endpoint_unsupported");

        let invalid_claude_mapping = vec![ModelMapping {
            from: "claude-unknown".into(),
            to: "claude-private-model".into(),
            label: None,
            supports_1m: None,
            ..Default::default()
        }];
        let input = build_input(
            "claude",
            "anthropic",
            &invalid_claude_mapping,
            &empty_headers,
        );
        assert_safe_build_error(&input, "deeplink_export.claude_models_unsupported");
    }

    #[test]
    fn rejects_claude_subagent_and_fallback_mappings_with_the_existing_code() {
        // Intentional lossy-export refusal, not a bug: the aiswitch:// link
        // format has fixed haiku/sonnet/opus query keys with no room for these,
        // and claude-fable-alias (a shipped role) is already refused the same way.
        // Do NOT "fix" this by widening the role table — inventing new query
        // keys makes links other tools silently mis-import.
        let empty_headers = json!({});

        for from in ["claude-subagent", "claude-model"] {
            let mappings = vec![ModelMapping {
                from: from.into(),
                to: "provider-model".into(),
                label: None,
                supports_1m: None,
                ..Default::default()
            }];
            let input = build_input("claude", "anthropic", &mappings, &empty_headers);
            assert_safe_build_error(&input, "deeplink_export.claude_models_unsupported");
        }
    }

    #[test]
    fn parses_claude_provider_with_role_models() {
        let url = "ccswitch://v1/import?resource=provider&app=claude&name=DeepLink%20Claude&endpoint=https%3A%2F%2Fapi.example.com%2Fv1&apiKey=sk-test-claude&sonnetModel=claude-sonnet-4&homepage=https%3A%2F%2Fexample.com&notes=demo";
        let parsed = parse_deeplink_url(url).expect("parse");
        assert_eq!(parsed.scheme, "ccswitch");
        assert_eq!(parsed.platform, "claude");
        assert_eq!(parsed.interface_format, "anthropic");
        assert_eq!(parsed.display_name, "DeepLink Claude");
        assert_eq!(parsed.base_url, "https://api.example.com/v1");
        assert_eq!(parsed.api_key, "sk-test-claude");
        assert!(parsed.api_key_masked.contains("***"));
        assert!(!parsed.source_url_sanitized.contains("sk-test-claude"));
        assert_eq!(parsed.homepage.as_deref(), Some("https://example.com"));
        assert_eq!(parsed.notes.as_deref(), Some("demo"));
        let mappings: Vec<ModelMapping> =
            serde_json::from_str(&parsed.model_mappings_json).unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].from, "claude-sonnet-alias");
        assert_eq!(mappings[0].to, "claude-sonnet-4");
        assert_eq!(mappings[0].label.as_deref(), Some("Sonnet"));
    }

    #[test]
    fn parses_aiswitch_codex_and_maps_model() {
        let url = "aiswitch://v1/import?resource=provider&app=codex&name=DeepLink%20Codex&endpoint=https%3A%2F%2Fapi.openai.example%2Fv1&apiKey=sk-test-codex&model=gpt-4.1";
        let parsed = parse_deeplink_url(url).expect("parse");
        assert_eq!(parsed.scheme, "aiswitch");
        assert_eq!(parsed.platform, "codex");
        assert_eq!(parsed.interface_format, "openai-responses");
        let mappings: Vec<ModelMapping> =
            serde_json::from_str(&parsed.model_mappings_json).unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].from, "gpt-5");
        assert_eq!(mappings[0].to, "gpt-4.1");
    }

    #[test]
    fn accepts_grok_and_xai_aliases() {
        for app in ["grok", "xai"] {
            let url = format!(
                "ccswitch://v1/import?resource=provider&app={app}&name=Grok%20One&endpoint=https%3A%2F%2Fapi.x.ai%2Fv1&apiKey=sk-grok-key&model=grok-4.5"
            );
            let parsed = parse_deeplink_url(&url).expect("parse");
            assert_eq!(parsed.platform, "grok");
            assert_eq!(parsed.interface_format, "openai");
            let mappings: Vec<ModelMapping> =
                serde_json::from_str(&parsed.model_mappings_json).unwrap();
            assert_eq!(mappings[0].from, "grok-3");
            assert_eq!(mappings[0].to, "grok-4.5");
        }
    }

    #[test]
    fn uses_first_valid_endpoint_from_csv() {
        let url = "ccswitch://v1/import?resource=provider&app=gemini&name=G&endpoint=not-a-url,https%3A%2F%2Fgood.example%2Fv1beta,https%3A%2F%2Fsecond.example&apiKey=sk-g";
        let parsed = parse_deeplink_url(url).expect("parse");
        assert_eq!(parsed.base_url, "https://good.example/v1beta");
        assert_eq!(parsed.interface_format, "gemini");
    }

    #[test]
    fn empty_model_fields_produce_empty_mappings() {
        let url = "ccswitch://v1/import?resource=provider&app=claude&name=NoMap&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-x";
        let parsed = parse_deeplink_url(url).expect("parse");
        assert_eq!(parsed.model_mappings_json, "[]");
    }

    #[test]
    fn rejects_bad_scheme_version_path_resource_and_app() {
        assert!(parse_deeplink_url(
            "http://v1/import?resource=provider&app=claude&name=A&endpoint=https://a&apiKey=sk"
        )
        .is_err());
        assert!(parse_deeplink_url(
            "ccswitch://v2/import?resource=provider&app=claude&name=A&endpoint=https://a&apiKey=sk"
        )
        .is_err());
        assert!(parse_deeplink_url(
            "ccswitch://v1/export?resource=provider&app=claude&name=A&endpoint=https://a&apiKey=sk"
        )
        .is_err());
        let resource_err = parse_deeplink_url(
            "ccswitch://v1/import?resource=mcp&app=claude&name=A&endpoint=https://a&apiKey=sk",
        )
        .unwrap_err();
        assert!(resource_err.contains("暂不支持"));
        assert!(parse_deeplink_url(
            "ccswitch://v1/import?resource=provider&app=opencode&name=A&endpoint=https://a&apiKey=sk"
        )
        .is_err());
        assert!(parse_deeplink_url(
            "ccswitch://v1/import?resource=provider&app=claude&name=A&endpoint=ftp://a&apiKey=sk"
        )
        .is_err());
        assert!(parse_deeplink_url(
            "ccswitch://v1/import?resource=provider&app=claude&name=A&endpoint=https://a"
        )
        .is_err());
    }

    #[test]
    fn partial_platform_deeplinks_return_capability_error() {
        for app in ["opencode", "openclaw", "hermes"] {
            let url = format!(
                "ccswitch://v1/import?resource=provider&app={app}&name=A&endpoint=https://a&apiKey=sk"
            );
            let error = parse_deeplink_url(&url)
                .expect_err("partial platforms do not support Deeplink import");
            assert!(error.contains("capability.unavailable"), "{app}: {error}");
        }
    }

    #[test]
    fn to_create_api_input_maps_fields() {
        let parsed = DeepLinkProviderImport {
            scheme: "aiswitch".into(),
            version: "v1".into(),
            resource: "provider".into(),
            app: "codex".into(),
            platform: "codex".into(),
            display_name: "N".into(),
            base_url: "https://api.example".into(),
            api_key_masked: "sk-t***odex".into(),
            api_key: "sk-test-codex".into(),
            interface_format: "openai-responses".into(),
            model_mappings_json: "[]".into(),
            homepage: None,
            notes: None,
            source_url_sanitized: "aiswitch://v1/import".into(),
        };
        let input = to_create_api_input(&parsed);
        assert_eq!(input.platform, "codex");
        assert_eq!(input.api_key, "sk-test-codex");
        assert_eq!(input.base_url, "https://api.example");
        assert_eq!(input.interface_format, "openai-responses");
        assert_eq!(input.model_mappings_json, "[]");
        assert!(input.api_key_field.is_none());
        assert!(input.preview_json.is_none());
        assert!(input.batch_id.is_none());
    }
}
