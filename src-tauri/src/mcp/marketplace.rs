//! MCP marketplace integration.
//!
//! The marketplace layer deliberately keeps provider-specific wire formats out
//! of the client adapters. Both providers are converted to the same canonical
//! MCP spec before the service writes it to local client configuration files.

use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::AppError;

use super::model::{
    LocalMcpServer, McpAppType, McpMarketplaceInstallOption, McpMarketplaceInstallParameter,
    McpMarketplaceItem, McpMarketplaceProvider, McpMarketplaceServerDetail,
};
use super::normalize::{canonicalize_spec, normalize_mcp_type};
use super::service;

const OFFICIAL: &str = "official_registry";
const SMITHERY: &str = "smithery";
const OFFICIAL_SERVERS_URL: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";
const SMITHERY_SERVERS_URL: &str = "https://api.smithery.ai/servers";

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Validation {
        code: "mcp.marketplace_invalid",
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

fn network(message: impl Into<String>) -> AppError {
    AppError::Validation {
        code: "mcp.marketplace_network",
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

fn not_found(message: impl Into<String>) -> AppError {
    AppError::Validation {
        code: "mcp.marketplace_not_found",
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

fn client() -> Result<Client, AppError> {
    HTTP_CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(25))
                .user_agent("ai-switch-mcp-market/1.0")
                .build()
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(network)
}

async fn send(request: reqwest::RequestBuilder, context: &str) -> Result<Response, AppError> {
    request
        .send()
        .await
        .map_err(|error| network(format!("{context}: {error}")))
}

async fn json<T: DeserializeOwned>(response: Response, context: &str) -> Result<T, AppError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| network(format!("{context}: could not read response: {error}")))?;
    if !status.is_success() {
        return Err(network(format!("{context}: HTTP {status}")));
    }
    serde_json::from_str(&body)
        .map_err(|error| network(format!("{context}: invalid JSON response: {error}")))
}

pub async fn list_marketplaces() -> Vec<McpMarketplaceProvider> {
    vec![
        McpMarketplaceProvider {
            id: OFFICIAL.to_string(),
            name: "Official MCP Registry".to_string(),
            description: "registry.modelcontextprotocol.io official MCP server registry"
                .to_string(),
        },
        McpMarketplaceProvider {
            id: SMITHERY.to_string(),
            name: "Smithery".to_string(),
            description: "smithery.ai MCP server marketplace".to_string(),
        },
    ]
}

#[derive(Debug, Deserialize)]
struct OfficialListResponse {
    #[serde(default)]
    servers: Vec<OfficialEntry>,
}

#[derive(Debug, Deserialize)]
struct OfficialEntry {
    server: OfficialServer,
    #[serde(default)]
    _meta: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OfficialServer {
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "websiteUrl")]
    website_url: Option<String>,
    #[serde(default)]
    repository: Option<OfficialRepository>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    icons: Vec<OfficialIcon>,
    #[serde(default)]
    remotes: Vec<OfficialTransport>,
    #[serde(default)]
    packages: Vec<OfficialPackage>,
}

#[derive(Debug, Deserialize)]
struct OfficialRepository {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OfficialIcon {
    #[serde(default)]
    src: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OfficialTransport {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: Option<Value>,
    #[serde(default)]
    variables: Option<Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct OfficialPackage {
    #[serde(default, rename = "registryType")]
    registry_type: String,
    identifier: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "runtimeHint")]
    runtime_hint: Option<String>,
    #[serde(default, rename = "runtimeArguments")]
    runtime_arguments: Vec<OfficialArgument>,
    #[serde(default, rename = "packageArguments")]
    package_arguments: Vec<OfficialArgument>,
    #[serde(default, rename = "environmentVariables")]
    environment_variables: Vec<OfficialParameter>,
    transport: OfficialTransport,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct OfficialArgument {
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default, rename = "isRequired")]
    required: Option<bool>,
    #[serde(default, rename = "valueHint")]
    placeholder: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct OfficialParameter {
    name: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default, rename = "isRequired")]
    required: Option<bool>,
    #[serde(default, rename = "isSecret")]
    secret: Option<bool>,
    #[serde(default, rename = "valueHint")]
    placeholder: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SmitheryListResponse {
    #[serde(default)]
    servers: Vec<SmitherySummary>,
}

#[derive(Debug, Deserialize, Clone)]
struct SmitherySummary {
    #[serde(rename = "qualifiedName")]
    qualified_name: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default, rename = "iconUrl")]
    icon_url: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    remote: bool,
    #[serde(default)]
    verified: bool,
    #[serde(default, rename = "useCount")]
    downloads: Option<u64>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default, rename = "isDeployed")]
    is_deployed: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SmitheryDetail {
    #[serde(rename = "qualifiedName")]
    qualified_name: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default, rename = "iconUrl")]
    icon_url: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default, rename = "deploymentUrl")]
    deployment_url: Option<String>,
    #[serde(default)]
    remote: bool,
    #[serde(default)]
    verified: bool,
    #[serde(default, rename = "useCount")]
    downloads: Option<u64>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default, rename = "isDeployed")]
    is_deployed: Option<bool>,
    #[serde(default)]
    connections: Vec<SmitheryConnection>,
}

#[derive(Debug, Deserialize)]
struct SmitheryConnection {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default, rename = "deploymentUrl")]
    deployment_url: Option<String>,
    #[serde(default, rename = "configSchema")]
    config_schema: Option<Value>,
}

fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => clean(Some(value)),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
        Value::Null => None,
    }
}

fn parameter_kind(format: Option<&str>) -> String {
    match format.unwrap_or("string").trim() {
        "boolean" => "boolean",
        "number" => "number",
        "integer" => "integer",
        "object" | "array" => "json",
        _ => "string",
    }
    .to_string()
}

fn secret_name(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("api_key")
        || key.ends_with("key")
}

fn transport_kind(raw: &str) -> Option<String> {
    normalize_mcp_type(raw).map(str::to_string)
}

fn official_verified(entry: &OfficialEntry) -> bool {
    entry
        ._meta
        .as_ref()
        .and_then(|meta| meta.get("io.modelcontextprotocol.registry/official"))
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("active"))
}

fn official_item(entry: &OfficialEntry) -> McpMarketplaceItem {
    let server = &entry.server;
    let homepage = clean(server.website_url.as_deref()).or_else(|| {
        server
            .repository
            .as_ref()
            .and_then(|repo| clean(repo.url.as_deref()))
    });
    let mut protocols = BTreeSet::new();
    for transport in &server.remotes {
        if let Some(kind) = transport_kind(&transport.kind) {
            protocols.insert(kind);
        }
    }
    for package in &server.packages {
        if let Some(kind) = transport_kind(&package.transport.kind) {
            protocols.insert(kind);
        }
    }
    McpMarketplaceItem {
        provider_id: OFFICIAL.to_string(),
        server_id: server.name.clone(),
        name: clean(server.title.as_deref()).unwrap_or_else(|| server.name.clone()),
        description: clean(server.description.as_deref())
            .unwrap_or_else(|| "No description".to_string()),
        homepage,
        remote: !server.remotes.is_empty(),
        verified: official_verified(entry),
        icon_url: server
            .icons
            .iter()
            .find_map(|icon| clean(icon.src.as_deref())),
        latest_version: clean(server.version.as_deref()),
        protocols: protocols.into_iter().collect(),
        owner: None,
        namespace: None,
        downloads: None,
        score: None,
        is_deployed: None,
    }
}

fn official_parameter(
    key: String,
    parameter: &OfficialParameter,
) -> McpMarketplaceInstallParameter {
    McpMarketplaceInstallParameter {
        label: key.clone(),
        key,
        description: clean(parameter.description.as_deref()),
        required: parameter.required.unwrap_or(false),
        secret: parameter.secret.unwrap_or(false) || secret_name(&parameter.name),
        kind: parameter_kind(parameter.format.as_deref()),
        default_value: parameter
            .value
            .as_deref()
            .or(parameter.default.as_deref())
            .and_then(|value| {
                serde_json::from_str(value)
                    .ok()
                    .or_else(|| Some(Value::String(value.to_string())))
            }),
        placeholder: clean(parameter.placeholder.as_deref()),
        enum_values: Vec::new(),
        location: None,
    }
}

fn argument_parameter(key: String, argument: &OfficialArgument) -> McpMarketplaceInstallParameter {
    McpMarketplaceInstallParameter {
        label: key.clone(),
        key,
        description: clean(argument.description.as_deref()),
        required: argument.required.unwrap_or(false),
        secret: false,
        kind: parameter_kind(argument.format.as_deref()),
        default_value: argument
            .value
            .as_deref()
            .or(argument.default.as_deref())
            .map(|value| Value::String(value.to_string())),
        placeholder: clean(argument.placeholder.as_deref()),
        enum_values: Vec::new(),
        location: None,
    }
}

fn option_id(source: &str, index: usize, protocol: &str) -> String {
    format!("{source}:{index}:{protocol}")
}

fn protocol_priority(protocol: &str) -> u8 {
    match normalize_mcp_type(protocol) {
        Some("stdio") => 0,
        Some("http") => 1,
        Some("sse") => 2,
        _ => 3,
    }
}

fn default_option(options: &[McpMarketplaceInstallOption]) -> Option<&McpMarketplaceInstallOption> {
    options
        .iter()
        .enumerate()
        .min_by_key(|(index, option)| (protocol_priority(&option.protocol), *index))
        .map(|(_, option)| option)
}

fn official_options(server: &OfficialServer) -> Vec<McpMarketplaceInstallOption> {
    let mut options = Vec::new();
    for (index, transport) in server.remotes.iter().enumerate() {
        let Some(protocol) = transport_kind(&transport.kind) else {
            continue;
        };
        let Some(url) = clean(transport.url.as_deref()) else {
            continue;
        };
        let mut spec = Map::new();
        spec.insert("type".to_string(), Value::String(protocol.clone()));
        spec.insert("url".to_string(), Value::String(url));
        let mut parameters = Vec::new();
        parameters.extend(parameter_entries(transport.headers.as_ref(), "header"));
        for raw in parameter_entries(transport.variables.as_ref(), "query") {
            if !parameters
                .iter()
                .any(|item: &McpMarketplaceInstallParameter| item.key == raw.key)
            {
                parameters.push(raw);
            }
        }
        options.push(McpMarketplaceInstallOption {
            id: option_id("official:remote", index, &protocol),
            protocol,
            label: "Remote transport".to_string(),
            description: None,
            spec: Value::Object(spec),
            parameters,
        });
    }
    for (index, package) in server.packages.iter().enumerate() {
        let Some(protocol) = transport_kind(&package.transport.kind) else {
            continue;
        };
        if protocol != "stdio" {
            continue;
        }
        let Some(runtime) = package_runtime(package) else {
            continue;
        };
        let mut parameters = Vec::new();
        for (index, argument) in package.runtime_arguments.iter().enumerate() {
            parameters.push(argument_parameter(
                format!("runtime_arguments.{index}"),
                argument,
            ));
        }
        for (index, argument) in package.package_arguments.iter().enumerate() {
            parameters.push(argument_parameter(
                format!("package_arguments.{index}"),
                argument,
            ));
        }
        for variable in &package.environment_variables {
            parameters.push(official_parameter(
                format!("env.{}", variable.name),
                variable,
            ));
        }
        let mut spec = Map::new();
        spec.insert("type".to_string(), Value::String("stdio".to_string()));
        spec.insert("command".to_string(), Value::String(runtime));
        let mut args = Vec::new();
        args.extend(package.runtime_arguments.iter().map(|argument| {
            Value::String(
                argument
                    .value
                    .as_deref()
                    .or(argument.default.as_deref())
                    .unwrap_or_default()
                    .to_string(),
            )
        }));
        args.push(Value::String(package_identifier(package)));
        args.extend(package.package_arguments.iter().map(|argument| {
            Value::String(
                argument
                    .value
                    .as_deref()
                    .or(argument.default.as_deref())
                    .unwrap_or_default()
                    .to_string(),
            )
        }));
        spec.insert("args".to_string(), Value::Array(args));
        options.push(McpMarketplaceInstallOption {
            id: option_id("official:package", index, "stdio"),
            protocol: "stdio".to_string(),
            label: format!("{} package", package.registry_type),
            description: None,
            spec: Value::Object(spec),
            parameters,
        });
    }
    options
}

fn parameter_entries(value: Option<&Value>, location: &str) -> Vec<McpMarketplaceInstallParameter> {
    let mut result = Vec::new();
    let Some(value) = value else { return result };
    let mut push = |key: String, raw: &Value| {
        let (description, required, secret, default_value) =
            raw.as_object()
                .map_or((None, false, secret_name(&key), raw.clone()), |object| {
                    (
                        clean(object.get("description").and_then(Value::as_str)),
                        object
                            .get("isRequired")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        object
                            .get("isSecret")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                            || secret_name(&key),
                        object
                            .get("default")
                            .cloned()
                            .or_else(|| object.get("value").cloned())
                            .unwrap_or(Value::Null),
                    )
                });
        result.push(McpMarketplaceInstallParameter {
            label: key.clone(),
            key,
            description,
            required,
            secret,
            kind: "string".to_string(),
            default_value: (!default_value.is_null()).then_some(default_value),
            placeholder: None,
            enum_values: Vec::new(),
            location: Some(location.to_string()),
        });
    };
    if let Some(array) = value.as_array() {
        for (index, raw) in array.iter().enumerate() {
            let key = raw
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{location}.{index}"));
            push(key, raw);
        }
    } else if let Some(object) = value.as_object() {
        for (key, raw) in object {
            push(key.clone(), raw);
        }
    }
    result
}

fn package_runtime(package: &OfficialPackage) -> Option<String> {
    clean(package.runtime_hint.as_deref()).or_else(|| match package.registry_type.as_str() {
        "npm" => Some("npx".to_string()),
        "pypi" => Some("uvx".to_string()),
        _ => None,
    })
}

fn package_identifier(package: &OfficialPackage) -> String {
    match clean(package.version.as_deref()) {
        Some(version) => format!("{}@{}", package.identifier, version),
        None => package.identifier.clone(),
    }
}

fn select_option<'a>(
    options: &'a [McpMarketplaceInstallOption],
    option_id: Option<&str>,
    protocol: Option<&str>,
) -> Result<&'a McpMarketplaceInstallOption, AppError> {
    if let Some(id) = option_id.filter(|value| !value.trim().is_empty()) {
        return options
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| not_found(format!("install option not found: {id}")));
    }
    if let Some(protocol) = protocol.and_then(normalize_mcp_type) {
        return options
            .iter()
            .find(|item| item.protocol == protocol)
            .ok_or_else(|| not_found(format!("no install option for protocol {protocol}")));
    }
    options
        .first()
        .ok_or_else(|| not_found("server does not expose an installable transport"))
}

fn apply_parameter_values(
    spec: &Value,
    option: &McpMarketplaceInstallOption,
    values: Option<&Value>,
) -> Result<Value, AppError> {
    let empty = Map::new();
    let values = match values {
        Some(value) => value
            .as_object()
            .ok_or_else(|| invalid("parameter_values must be a JSON object"))?,
        None => &empty,
    };
    let mut output = spec.clone();
    for parameter in &option.parameters {
        let Some(value) = values
            .get(&parameter.key)
            .or(parameter.default_value.as_ref())
        else {
            if parameter.required {
                return Err(invalid(format!(
                    "missing required parameter {}",
                    parameter.key
                )));
            }
            continue;
        };
        let Some(text) = value_text(value) else {
            continue;
        };
        let location = parameter.location.as_deref();
        if parameter.key.starts_with("env.") {
            output["env"][parameter.key.trim_start_matches("env.")] = Value::String(text);
        } else if let Some((prefix, raw_index)) = parameter
            .key
            .split_once('.')
            .filter(|(prefix, _)| *prefix == "runtime_arguments" || *prefix == "package_arguments")
        {
            let index = raw_index.parse::<usize>().unwrap_or(usize::MAX);
            let offset = if prefix == "package_arguments" {
                option
                    .parameters
                    .iter()
                    .filter(|item| item.key.starts_with("runtime_arguments."))
                    .count()
                    + 1
            } else {
                0
            };
            let index = index.saturating_add(offset);
            if let Some(args) = output.get_mut("args").and_then(Value::as_array_mut) {
                if index < args.len() {
                    args[index] = Value::String(text);
                } else {
                    args.push(Value::String(text));
                }
            }
        } else if location == Some("header") {
            output["headers"][parameter.key.clone()] = Value::String(text);
        } else if location == Some("query") {
            let url = output
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let separator = if url.contains('?') { '&' } else { '?' };
            output["url"] = Value::String(format!(
                "{url}{separator}{}={}",
                parameter.key,
                url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>()
            ));
        }
    }
    canonicalize_spec(&output, "marketplace install")
}

pub async fn search(
    provider_id: String,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<McpMarketplaceItem>, AppError> {
    let provider_id = provider_id.trim().to_ascii_lowercase();
    let query = query.unwrap_or_default();
    let limit = limit.unwrap_or(30).clamp(1, 100);
    let client = client()?;
    match provider_id.as_str() {
        OFFICIAL => {
            let response = send(
                client.get(OFFICIAL_SERVERS_URL).query(&[
                    ("limit", limit.to_string()),
                    ("version", "latest".to_string()),
                    ("search", query.clone()),
                ]),
                "official registry request",
            )
            .await?;
            let payload: OfficialListResponse =
                json(response, "official registry response").await?;
            Ok(payload.servers.iter().map(official_item).collect())
        }
        SMITHERY => {
            let response = send(
                client
                    .get(SMITHERY_SERVERS_URL)
                    .query(&[("q", query.as_str()), ("limit", &limit.to_string())]),
                "Smithery request",
            )
            .await?;
            let payload: SmitheryListResponse = json(response, "Smithery response").await?;
            Ok(payload
                .servers
                .into_iter()
                .map(|item| McpMarketplaceItem {
                    provider_id: SMITHERY.to_string(),
                    server_id: item.qualified_name,
                    name: item.display_name,
                    description: clean(item.description.as_deref())
                        .unwrap_or_else(|| "No description".to_string()),
                    homepage: clean(item.homepage.as_deref()),
                    remote: item.remote,
                    verified: item.verified,
                    icon_url: clean(item.icon_url.as_deref()),
                    latest_version: None,
                    protocols: if item.remote {
                        vec!["http".to_string()]
                    } else {
                        vec!["stdio".to_string()]
                    },
                    owner: clean(item.owner.as_deref()),
                    namespace: clean(item.namespace.as_deref()),
                    downloads: item.downloads,
                    score: item.score,
                    is_deployed: item.is_deployed,
                })
                .collect())
        }
        _ => Err(invalid(format!(
            "unsupported marketplace provider: {provider_id}"
        ))),
    }
}

fn official_detail_url(server_id: &str) -> Result<String, AppError> {
    let mut url = url::Url::parse("https://registry.modelcontextprotocol.io/v0.1/servers/")
        .map_err(|error| invalid(error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| invalid("invalid official registry URL"))?
        .push(server_id)
        .push("versions")
        .push("latest");
    Ok(url.to_string())
}

async fn fetch_official(server_id: &str) -> Result<OfficialEntry, AppError> {
    let response = send(
        client()?.get(official_detail_url(server_id)?),
        "official detail request",
    )
    .await?;
    json(response, "official detail response").await
}

fn smithery_detail_url(server_id: &str) -> Result<url::Url, AppError> {
    let mut url = url::Url::parse(&format!("{SMITHERY_SERVERS_URL}/"))
        .map_err(|error| invalid(error.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| invalid("invalid Smithery URL"))?
        .push(server_id.trim());
    Ok(url)
}

async fn fetch_smithery(server_id: &str) -> Result<SmitheryDetail, AppError> {
    let response = send(
        client()?.get(smithery_detail_url(server_id)?),
        "Smithery detail request",
    )
    .await?;
    json(response, "Smithery detail response").await
}

fn smithery_protocol(connection: &SmitheryConnection) -> String {
    match normalize_mcp_type(&connection.kind) {
        Some("sse") => "sse".to_string(),
        _ => "http".to_string(),
    }
}

fn smithery_parameters(connection: &SmitheryConnection) -> Vec<McpMarketplaceInstallParameter> {
    let Some(schema) = connection.config_schema.as_ref().and_then(Value::as_object) else {
        return Vec::new();
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(key, value)| {
                    let prop = value.as_object();
                    let secret = prop
                        .and_then(|item| item.get("writeOnly"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        || secret_name(key);
                    McpMarketplaceInstallParameter {
                        key: key.clone(),
                        label: key.clone(),
                        description: prop
                            .and_then(|item| item.get("description"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        required: required.contains(key.as_str()),
                        secret,
                        kind: parameter_kind(
                            prop.and_then(|item| item.get("type"))
                                .and_then(Value::as_str),
                        ),
                        default_value: prop.and_then(|item| item.get("default")).cloned(),
                        placeholder: None,
                        enum_values: prop
                            .and_then(|item| item.get("enum"))
                            .and_then(Value::as_array)
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        location: Some(
                            if prop
                                .and_then(|item| item.get("x-from"))
                                .and_then(Value::as_str)
                                .is_some_and(|value| value.eq_ignore_ascii_case("header"))
                                || secret
                            {
                                "header"
                            } else {
                                "query"
                            }
                            .to_string(),
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn smithery_options(detail: &SmitheryDetail) -> Vec<McpMarketplaceInstallOption> {
    detail
        .connections
        .iter()
        .enumerate()
        .filter_map(|(index, connection)| {
            let url = clean(connection.deployment_url.as_deref())
                .or_else(|| clean(detail.deployment_url.as_deref()))?;
            let protocol = smithery_protocol(connection);
            let spec = canonicalize_spec(
                &serde_json::json!({"type": protocol, "url": url}),
                "Smithery connection",
            )
            .ok()?;
            Some(McpMarketplaceInstallOption {
                id: option_id("smithery:connection", index, &protocol),
                protocol: protocol.clone(),
                label: format!("{protocol} connection {}", index + 1),
                description: clean(connection.deployment_url.as_deref()),
                spec,
                parameters: smithery_parameters(connection),
            })
        })
        .collect()
}

pub async fn get_detail(
    provider_id: String,
    server_id: String,
) -> Result<McpMarketplaceServerDetail, AppError> {
    let provider_id = provider_id.trim().to_ascii_lowercase();
    match provider_id.as_str() {
        OFFICIAL => {
            let entry = fetch_official(&server_id).await?;
            let options = official_options(&entry.server);
            let selected = default_option(&options)
                .ok_or_else(|| not_found("official server has no installable transport"))?;
            let item = official_item(&entry);
            Ok(McpMarketplaceServerDetail {
                provider_id: OFFICIAL.to_string(),
                server_id: item.server_id,
                name: item.name,
                description: item.description,
                homepage: item.homepage,
                remote: item.remote,
                verified: item.verified,
                icon_url: item.icon_url,
                latest_version: item.latest_version,
                protocols: item.protocols,
                owner: item.owner,
                namespace: item.namespace,
                downloads: item.downloads,
                score: item.score,
                is_deployed: item.is_deployed,
                default_option_id: Some(selected.id.clone()),
                spec: selected.spec.clone(),
                install_options: options,
            })
        }
        SMITHERY => {
            let detail = fetch_smithery(&server_id).await?;
            let options = smithery_options(&detail);
            let selected = default_option(&options)
                .ok_or_else(|| not_found("Smithery server has no installable connection"))?;
            Ok(McpMarketplaceServerDetail {
                provider_id: SMITHERY.to_string(),
                server_id: detail.qualified_name.clone(),
                name: detail.display_name.clone(),
                description: clean(detail.description.as_deref())
                    .unwrap_or_else(|| "No description".to_string()),
                homepage: clean(detail.homepage.as_deref()),
                remote: detail.remote,
                verified: detail.verified,
                icon_url: clean(detail.icon_url.as_deref()),
                latest_version: None,
                protocols: options.iter().map(|item| item.protocol.clone()).collect(),
                owner: clean(detail.owner.as_deref()),
                namespace: clean(detail.namespace.as_deref()),
                downloads: detail.downloads,
                score: detail.score,
                is_deployed: detail.is_deployed,
                default_option_id: Some(selected.id.clone()),
                spec: selected.spec.clone(),
                install_options: options,
            })
        }
        _ => Err(invalid(format!(
            "unsupported marketplace provider: {provider_id}"
        ))),
    }
}

pub async fn install(
    provider_id: String,
    server_id: String,
    apps: Vec<McpAppType>,
    option_id: Option<String>,
    protocol: Option<String>,
    parameter_values: Option<Value>,
) -> Result<LocalMcpServer, AppError> {
    if apps.is_empty() {
        return Err(invalid("at least one target client is required"));
    }
    let detail = get_detail(provider_id.clone(), server_id.clone()).await?;
    let option = select_option(
        &detail.install_options,
        option_id.as_deref(),
        protocol.as_deref(),
    )?;
    let spec = apply_parameter_values(&option.spec, option, parameter_values.as_ref())?;
    service::upsert_local_server(server_id, spec, apps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_server_path_encodes_qualified_names_as_one_segment() {
        let url = official_detail_url("com.example/my-server").unwrap();
        assert!(url.contains("com.example%2Fmy-server"), "{url}");
    }

    #[test]
    fn smithery_server_path_encodes_qualified_names_as_one_segment() {
        let url = smithery_detail_url("namespace/server?x").unwrap();
        assert!(url.as_str().contains("namespace%2Fserver%3Fx"), "{url}");
    }

    #[test]
    fn parameter_values_are_applied_to_headers_and_query_parameters() {
        let option = McpMarketplaceInstallOption {
            id: "smithery:connection:0:http".to_string(),
            protocol: "http".to_string(),
            label: "http".to_string(),
            description: None,
            spec: serde_json::json!({"type":"http","url":"https://example.test/mcp"}),
            parameters: vec![
                McpMarketplaceInstallParameter {
                    key: "X-Token".to_string(),
                    label: "X-Token".to_string(),
                    description: None,
                    required: true,
                    secret: true,
                    kind: "string".to_string(),
                    default_value: None,
                    placeholder: None,
                    enum_values: Vec::new(),
                    location: Some("header".to_string()),
                },
                McpMarketplaceInstallParameter {
                    key: "workspace".to_string(),
                    label: "workspace".to_string(),
                    description: None,
                    required: true,
                    secret: false,
                    kind: "string".to_string(),
                    default_value: None,
                    placeholder: None,
                    enum_values: Vec::new(),
                    location: Some("query".to_string()),
                },
            ],
        };
        let spec = apply_parameter_values(
            &option.spec,
            &option,
            Some(&serde_json::json!({"X-Token":"secret", "workspace":"demo"})),
        )
        .unwrap();
        assert_eq!(spec["headers"]["X-Token"], "secret");
        assert_eq!(spec["url"], "https://example.test/mcp?workspace=demo");
    }

    #[test]
    fn required_parameters_are_rejected_without_values() {
        let option = McpMarketplaceInstallOption {
            id: "test".to_string(),
            protocol: "http".to_string(),
            label: "http".to_string(),
            description: None,
            spec: serde_json::json!({"type":"http","url":"https://example.test/mcp"}),
            parameters: vec![McpMarketplaceInstallParameter {
                key: "token".to_string(),
                label: "token".to_string(),
                description: None,
                required: true,
                secret: true,
                kind: "string".to_string(),
                default_value: None,
                placeholder: None,
                enum_values: Vec::new(),
                location: Some("header".to_string()),
            }],
        };
        assert!(apply_parameter_values(&option.spec, &option, None).is_err());
    }
}
