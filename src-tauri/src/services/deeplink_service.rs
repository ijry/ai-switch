use crate::models::route_credential::{CreateApiRouteCredentialInput, ModelMapping};
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
    let (platform, interface_format) = map_app(&app)?;
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
        api_key_field: None,
        preview_json: None,
        batch_id: None,
        responses_custom_tool_compat: None,
    }
}

pub fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    if trimmed.len() <= 8 {
        return format!("{}***", &trimmed[..trimmed.len().min(2)]);
    }
    format!(
        "{}***{}",
        &trimmed[..4],
        &trimmed[trimmed.len().saturating_sub(4)..]
    )
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

fn map_app(app: &str) -> Result<(String, String), String> {
    match app {
        "claude" => Ok(("claude".into(), "anthropic".into())),
        "codex" => Ok(("codex".into(), "openai-responses".into())),
        "gemini" => Ok(("gemini".into(), "gemini".into())),
        "grok" | "xai" => Ok(("grok".into(), "openai".into())),
        "opencode" | "openclaw" => Err(format!("不支持的应用: {app}")),
        other => Err(format!("不支持的应用: {other}")),
    }
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
            "claude-haiku-4-5",
            "Haiku",
        );
        push_claude_mapping(
            &mut mappings,
            params,
            "sonnetModel",
            "claude-sonnet-5",
            "Sonnet",
        );
        push_claude_mapping(
            &mut mappings,
            params,
            "opusModel",
            "claude-opus-4-8",
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
        assert_eq!(mappings[0].from, "claude-sonnet-5");
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
