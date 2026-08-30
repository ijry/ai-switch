use self::sub2api_import_service::{is_sub2api_shape_error, parse_sub2api_text};
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::batch::NewBatch;
use crate::models::platform::{ApiDialect, PlatformId, PlatformOperation};
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, CopyRouteCredentialInput, CreateApiRouteCredentialInput,
    ImportOfficialFilesInput, ImportOfficialTextInput, ModelMapping, ReorderRouteCredentialInput,
    RouteCredential, RouteCredentialFailurePolicy, RouteCredentialImportFailure,
    RouteCredentialImportResult, RouteCredentialPage, RouteCredentialPageRequest,
    UpdateRouteCredentialInput,
};
use crate::models::route_credential_transfer::TransferPlatformChoice;
use crate::models::route_pool::FetchedRouteModel;
use crate::services::cpa_import_service::{parse_cpa_text, ParsedOfficialCredential};
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_credential_activity::RouteCredentialActivityRegistry;
use crate::services::route_credential_transfer_import_service::{
    normalize_transfer_items, NormalizedImportItem,
};
use crate::services::route_preview_service::RoutePreviewService;
use chrono::Utc;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use url::Url;

#[path = "sub2api_import_service.rs"]
mod sub2api_import_service;

pub struct RouteCredentialService;

enum ParsedBatchCredential {
    Official(ParsedOfficialCredential),
    Api(NormalizedImportItem),
}

impl RouteCredentialService {
    pub async fn list(
        pool: &SqlitePool,
        platform: String,
    ) -> Result<Vec<RouteCredential>, AppError> {
        let platform = PlatformId::parse(&platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        RouteCredentialRepository::list_by_platform(pool, platform.as_str()).await
    }

    pub async fn list_with_activity(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        platform: String,
    ) -> Result<Vec<RouteCredential>, AppError> {
        let credentials = Self::list(pool, platform).await?;
        Ok(apply_activity_counts(credentials, activity))
    }

    pub async fn get(pool: &SqlitePool, id: String) -> Result<RouteCredential, AppError> {
        RouteCredentialRepository::get(pool, &id).await
    }

    pub async fn get_with_activity(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        id: String,
    ) -> Result<RouteCredential, AppError> {
        let mut credential = Self::get(pool, id).await?;
        credential.active_request_count = activity.snapshot(&credential.id);
        Ok(credential)
    }

    pub async fn page(
        pool: &SqlitePool,
        request: RouteCredentialPageRequest,
    ) -> Result<RouteCredentialPage, AppError> {
        let platform = PlatformId::parse(&request.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        RouteCredentialRepository::page(
            pool,
            RouteCredentialPageRequest {
                platform: platform.as_str().to_string(),
                ..request
            },
        )
        .await
    }

    pub async fn page_with_activity(
        pool: &SqlitePool,
        activity: &RouteCredentialActivityRegistry,
        request: RouteCredentialPageRequest,
    ) -> Result<RouteCredentialPage, AppError> {
        let mut page = Self::page(pool, request).await?;
        page.items = apply_activity_counts(page.items, activity);
        Ok(page)
    }

    pub async fn reorder(
        pool: &SqlitePool,
        input: ReorderRouteCredentialInput,
    ) -> Result<RouteCredentialPage, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        RouteCredentialRepository::reorder(
            pool,
            ReorderRouteCredentialInput {
                platform: platform.as_str().to_string(),
                ..input
            },
        )
        .await
    }

    pub async fn create_api(
        pool: &SqlitePool,
        input: CreateApiRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        let platform = platform.as_str();
        validate_required("display_name", &input.display_name)?;
        validate_required("api_key", &input.api_key)?;
        validate_required("base_url", &input.base_url)?;
        validate_interface_format(&input.interface_format)?;
        validate_model_mappings(&input.model_mappings_json)?;
        let fetched_models = parse_fetched_models_json(input.fetched_models_json.as_deref())?;
        let api_key_field =
            validate_api_key_field(input.api_key_field.as_deref(), &input.interface_format)?;

        let secret_payload_json = json!({ "api_key": input.api_key.trim() }).to_string();
        let mut config = json!({
            "base_url": input.base_url.trim(),
            "interface_format": input.interface_format,
            "model_mappings": serde_json::from_str::<serde_json::Value>(&input.model_mappings_json)?,
            "fetched_models": fetched_models,
            "responses_custom_tool_compat": input.responses_custom_tool_compat.unwrap_or(false),
        });
        if let Some(api_key_field) = api_key_field {
            config["api_key_field"] = json!(api_key_field);
        }
        if let Some(user_agent) = input
            .user_agent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            config["headers"] = json!({ "User-Agent": user_agent });
        }
        let config_json = config.to_string();
        let preview_json = input.preview_json.unwrap_or_else(|| {
            RoutePreviewService::generate(&platform, "api", &secret_payload_json, &config_json)
        });

        RouteCredentialRepository::create(
            pool,
            &platform,
            "api",
            input.display_name.trim(),
            None,
            "ok",
            input.batch_id,
            &secret_payload_json,
            &config_json,
            &preview_json,
        )
        .await
    }

    pub async fn import_official_text(
        pool: &SqlitePool,
        input: ImportOfficialTextInput,
    ) -> Result<RouteCredentialImportResult, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::OfficialImport)?;
        let platform = platform.as_str();
        let batch_id = ensure_required_batch(pool, input.batch_name).await?;
        let parsed = parse_batch_credentials_text(platform, &input.text)?;
        let mut imported = Vec::with_capacity(parsed.len());

        for credential in parsed {
            imported
                .push(create_batch_credential(pool, platform, batch_id.clone(), credential).await?);
        }

        Ok(RouteCredentialImportResult {
            imported,
            failed: Vec::new(),
        })
    }

    pub async fn import_official_files(
        pool: &SqlitePool,
        input: ImportOfficialFilesInput,
    ) -> Result<RouteCredentialImportResult, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::OfficialImport)?;
        let platform = platform.as_str();
        let batch_id = ensure_required_batch(pool, input.batch_name).await?;
        let mut imported = Vec::new();
        let mut failed = Vec::new();

        for path in input.file_paths {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match parse_batch_credentials_file(platform, &path, &content) {
                    Ok(credentials) => {
                        for credential in credentials {
                            imported.push(
                                create_batch_credential(
                                    pool,
                                    platform,
                                    batch_id.clone(),
                                    credential,
                                )
                                .await?,
                            );
                        }
                    }
                    Err(err) => failed.push(RouteCredentialImportFailure {
                        label: path,
                        error: err.to_string(),
                    }),
                },
                Err(err) => failed.push(RouteCredentialImportFailure {
                    label: path,
                    error: err.to_string(),
                }),
            }
        }

        Ok(RouteCredentialImportResult { imported, failed })
    }

    pub async fn update(
        pool: &SqlitePool,
        id: String,
        input: UpdateRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> {
        validate_required("display_name", &input.display_name)?;
        validate_route_credential_status(&input.status)?;
        validate_route_priority(input.route_priority)?;
        validate_max_concurrency(input.max_concurrency)?;
        validate_failure_policy_config(&input.config_json)?;
        RouteCredentialRepository::update(pool, &id, &input).await
    }

    pub async fn copy(pool: &SqlitePool, id: String) -> Result<RouteCredential, AppError> {
        Self::copy_with_options(pool, id, CopyRouteCredentialInput::default()).await
    }

    pub async fn copy_with_options(
        pool: &SqlitePool,
        id: String,
        input: CopyRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> {
        let source = RouteCredentialRepository::get(pool, &id).await?;
        let source_platform = PlatformId::parse(&source.platform)?;
        let target_platform = input
            .target_platform
            .as_deref()
            .map(PlatformId::parse)
            .transpose()?
            .unwrap_or(source_platform);
        PlatformCapabilityService::require(target_platform, PlatformOperation::RouteCredentials)?;
        let cross_platform = target_platform != source_platform;
        let api_key_override = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if source.kind != "api" && cross_platform {
            return Err(AppError::Validation {
                code: "validation.official_cross_platform_copy",
                message: "Official accounts can only be copied within the same platform"
                    .to_string(),
                details: Some(format!(
                    "{} -> {}",
                    source_platform.as_str(),
                    target_platform.as_str()
                )),
                recoverable: true,
            });
        }
        if source.kind != "api" && api_key_override.is_some() {
            return Err(AppError::Validation {
                code: "validation.copy_api_key_unsupported",
                message: "API Key override is only supported for API accounts".to_string(),
                details: Some(source.kind.clone()),
                recoverable: true,
            });
        }

        let (secret_payload_json, config_json, preview_json) = if source.kind == "api" {
            copied_api_payload(&source, target_platform, cross_platform, api_key_override)?
        } else {
            (
                source.secret_payload_json.clone(),
                source.config_json.clone(),
                source.preview_json.clone(),
            )
        };
        let display_name = duplicated_display_name(&source.display_name);
        let created = RouteCredentialRepository::create_with_routing_settings(
            pool,
            target_platform.as_str(),
            &source.kind,
            &display_name,
            (!cross_platform).then_some(source.email.clone()).flatten(),
            "ok",
            (!cross_platform)
                .then_some(source.batch_id.clone())
                .flatten(),
            &secret_payload_json,
            &config_json,
            &preview_json,
            source.route_priority,
            source.max_concurrency,
        )
        .await?;

        // Preserve the source's compute-pool membership: a copy made from the
        // "算力池" view should stay in the pool rather than dropping to "未入池".
        if !cross_platform {
            let source_in_pool = RoutePoolRepository::pool_membership_map(
                pool,
                &source.platform,
                std::slice::from_ref(&source.id),
            )
            .await?
            .contains(&source.id);
            if source_in_pool {
                RoutePoolRepository::append_members(
                    pool,
                    target_platform.as_str(),
                    std::slice::from_ref(&created.id),
                )
                .await?;
            }
        }

        Ok(created)
    }

    pub async fn delete(pool: &SqlitePool, id: String) -> Result<(), AppError> {
        RouteCredentialRepository::delete(pool, &id).await
    }

    pub async fn archive(pool: &SqlitePool, ids: Vec<String>) -> Result<(), AppError> {
        RouteCredentialRepository::set_archived(pool, &ids, true).await
    }

    pub async fn restore(pool: &SqlitePool, ids: Vec<String>) -> Result<(), AppError> {
        RouteCredentialRepository::set_archived(pool, &ids, false).await
    }

    pub async fn set_statuses(
        pool: &SqlitePool,
        ids: Vec<String>,
        status: String,
    ) -> Result<(), AppError> {
        RouteCredentialRepository::set_statuses(pool, &ids, &status).await
    }
}

fn duplicated_display_name(name: &str) -> String {
    let base = name.trim();
    let stamp = Utc::now().format("%Y-%m-%d").to_string();
    if base.is_empty() {
        format!("copy {stamp}")
    } else {
        format!("{base} {stamp}")
    }
}

fn copied_api_payload(
    source: &RouteCredential,
    target_platform: PlatformId,
    cross_platform: bool,
    api_key_override: Option<&str>,
) -> Result<(String, String, String), AppError> {
    if !cross_platform && api_key_override.is_none() {
        return Ok((
            source.secret_payload_json.clone(),
            source.config_json.clone(),
            source.preview_json.clone(),
        ));
    }

    let source_secret = parse_copy_object(&source.secret_payload_json, "secret_payload_json")?;
    let api_key = api_key_override
        .or_else(|| source_secret.get("api_key").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| copy_source_error("api_key", "API account has no API Key"))?;

    if !cross_platform {
        let mut secret = source_secret;
        secret.insert("api_key".to_string(), json!(&api_key));
        let secret_payload_json = Value::Object(secret).to_string();
        let preview_json = RoutePreviewService::generate(
            target_platform.as_str(),
            "api",
            &secret_payload_json,
            &source.config_json,
        );
        return Ok((
            secret_payload_json,
            source.config_json.clone(),
            preview_json,
        ));
    }

    let source_config = parse_copy_object(&source.config_json, "config_json")?;
    let source_interface_format = source_config
        .get("interface_format")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            copy_source_error("interface_format", "API account has no interface format")
        })?;
    let source_dialect = ApiDialect::parse(source_interface_format)?;
    let target_default_dialect = target_platform.default_api_credential_dialect();
    let target_dialect = target_default_dialect.unwrap_or(source_dialect);
    let source_base_url = source_config
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| copy_source_error("base_url", "API account has no Base URL"))?;
    let base_url = match target_default_dialect {
        Some(dialect) => convert_copy_base_url(source_base_url, dialect)?,
        None => source_base_url.trim().to_string(),
    };

    let mut config = Map::from_iter([
        ("base_url".to_string(), json!(base_url)),
        (
            "interface_format".to_string(),
            json!(target_dialect.as_str()),
        ),
    ]);
    for key in ["headers", "failure_policy", "recovery"] {
        if let Some(value) = source_config.get(key) {
            config.insert(key.to_string(), value.clone());
        }
    }

    let secret_payload_json = json!({ "api_key": api_key }).to_string();
    let config_json = Value::Object(config).to_string();
    let preview_json = RoutePreviewService::generate(
        target_platform.as_str(),
        "api",
        &secret_payload_json,
        &config_json,
    );
    Ok((secret_payload_json, config_json, preview_json))
}

fn parse_copy_object(value: &str, field: &'static str) -> Result<Map<String, Value>, AppError> {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| copy_source_error(field, "Account data is not a JSON object"))
}

fn copy_source_error(field: &'static str, message: &'static str) -> AppError {
    AppError::Validation {
        code: "validation.copy_source",
        message: message.to_string(),
        details: Some(field.to_string()),
        recoverable: true,
    }
}

fn convert_copy_base_url(value: &str, target_dialect: ApiDialect) -> Result<String, AppError> {
    let trimmed = value.trim();
    if matches!(target_dialect, ApiDialect::Gemini) {
        return Ok(trimmed.to_string());
    }

    let mut url = Url::parse(trimmed).map_err(|error| AppError::Validation {
        code: "validation.copy_base_url",
        message: "Base URL cannot be converted for the target platform".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    let current_path = url.path().trim_end_matches('/');
    let path = match target_dialect {
        ApiDialect::Anthropic => {
            if current_path
                .rsplit('/')
                .next()
                .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
            {
                current_path[..current_path.len() - "/v1".len()].to_string()
            } else {
                current_path.to_string()
            }
        }
        ApiDialect::OpenAi | ApiDialect::OpenAiResponses => {
            if current_path
                .rsplit('/')
                .next()
                .is_some_and(|segment| segment.eq_ignore_ascii_case("v1"))
            {
                current_path.to_string()
            } else if current_path.is_empty() {
                "/v1".to_string()
            } else {
                format!("{current_path}/v1")
            }
        }
        ApiDialect::Gemini => unreachable!(),
    };
    url.set_path(if path.is_empty() { "/" } else { &path });
    let rendered = url.to_string();
    if path.is_empty() && url.query().is_none() && url.fragment().is_none() {
        Ok(rendered.trim_end_matches('/').to_string())
    } else {
        Ok(rendered)
    }
}

async fn ensure_required_batch(
    pool: &SqlitePool,
    batch_name: Option<String>,
) -> Result<Option<String>, AppError> {
    let Some(name) = batch_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Err(AppError::Validation {
            code: "validation.batch_name_required",
            message: "Batch name is required".to_string(),
            details: None,
            recoverable: true,
        });
    };

    let batch = BatchRepository::create(
        pool,
        NewBatch {
            name,
            source: "route_credential_import".to_string(),
            notes: None,
        },
    )
    .await?;

    Ok(Some(batch.id))
}

fn validate_required(field: &'static str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::Validation {
            code: "validation.required",
            message: format!("{field} is required"),
            details: Some(field.to_string()),
            recoverable: true,
        });
    }
    Ok(())
}

fn apply_activity_counts(
    credentials: Vec<RouteCredential>,
    activity: &RouteCredentialActivityRegistry,
) -> Vec<RouteCredential> {
    credentials
        .into_iter()
        .map(|mut credential| {
            credential.active_request_count = activity.snapshot(&credential.id);
            credential
        })
        .collect()
}

fn validate_route_credential_status(value: &str) -> Result<(), AppError> {
    match value {
        "ok" | "warning" | "error" | "revoked" | "paused" => Ok(()),
        _ => Err(AppError::Validation {
            code: "validation.route_credential_status",
            message: "Route credential status is not supported".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }),
    }
}

fn validate_route_priority(value: i64) -> Result<i64, AppError> {
    if (1..=5).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::Validation {
            code: "validation.route_credential_priority",
            message: "Route priority must be between 1 and 5".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        })
    }
}

fn validate_max_concurrency(value: i64) -> Result<i64, AppError> {
    if value >= 1 {
        Ok(value)
    } else {
        Err(AppError::Validation {
            code: "validation.route_credential_concurrency",
            message: "Max concurrency must be at least 1".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        })
    }
}

fn validate_failure_policy_config(config_json: &str) -> Result<(), AppError> {
    let Ok(config) = serde_json::from_str::<Value>(config_json) else {
        return Ok(());
    };
    if config.get("failure_policy").is_none() {
        return Ok(());
    }
    RouteCredentialFailurePolicy::from_config_value(&config).map_err(|message| {
        AppError::Validation {
            code: "validation.route_credential_failure_policy",
            message: "Route credential failure policy is invalid".to_string(),
            details: Some(message),
            recoverable: true,
        }
    })?;
    Ok(())
}

fn validate_interface_format(value: &str) -> Result<(), AppError> {
    match value {
        "openai" | "openai-responses" | "anthropic" | "gemini" => Ok(()),
        _ => Err(AppError::Validation {
            code: "validation.interface_format",
            message: "Interface format is not supported".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }),
    }
}

fn validate_api_key_field(
    value: Option<&str>,
    interface_format: &str,
) -> Result<Option<&'static str>, AppError> {
    let Some(value) = value.map(str::trim).filter(|item| !item.is_empty()) else {
        return Ok(None);
    };
    if !is_anthropic_interface_format(interface_format) {
        return Err(AppError::Validation {
            code: "validation.api_key_field",
            message: "api_key_field is only supported for Anthropic interface formats".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        });
    }
    normalize_anthropic_api_key_field(Some(value))
        .map(Some)
        .map_err(|err| AppError::Validation {
            code: "validation.api_key_field",
            message: err,
            details: Some(value.to_string()),
            recoverable: true,
        })
}

fn is_anthropic_interface_format(value: &str) -> bool {
    value == "anthropic"
}

fn parse_official_credentials_text(
    platform: &str,
    text: &str,
) -> Result<Vec<ParsedOfficialCredential>, AppError> {
    match parse_sub2api_text(platform, text) {
        Ok(credentials) => Ok(credentials),
        Err(err) if is_sub2api_shape_error(&err) => parse_cpa_text(platform, text),
        Err(err) => Err(err),
    }
}

fn parse_batch_credentials_text(
    platform: &str,
    text: &str,
) -> Result<Vec<ParsedBatchCredential>, AppError> {
    let value: Value = serde_json::from_str(text)?;
    let items = match value {
        Value::Object(object) if object.contains_key("accounts") => {
            return parse_official_credentials_text(platform, text).map(|credentials| {
                credentials
                    .into_iter()
                    .map(ParsedBatchCredential::Official)
                    .collect()
            });
        }
        Value::Object(object) => vec![object],
        Value::Array(items) => items
            .into_iter()
            .map(|item| {
                item.as_object()
                    .cloned()
                    .ok_or_else(|| AppError::Validation {
                        code: "validation.cpa_entry_object",
                        message: "CPA array entries must be objects".to_string(),
                        details: None,
                        recoverable: true,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return parse_official_credentials_text(platform, text).map(|credentials| {
                credentials
                    .into_iter()
                    .map(ParsedBatchCredential::Official)
                    .collect()
            });
        }
    };

    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        if is_api_batch_item(&item) {
            parsed.push(ParsedBatchCredential::Api(normalize_api_batch_item(
                platform, &item,
            )?));
            continue;
        }

        let item_text = Value::Object(item).to_string();
        let credentials = parse_official_credentials_text(platform, &item_text)?;
        parsed.extend(credentials.into_iter().map(ParsedBatchCredential::Official));
    }
    Ok(parsed)
}

fn parse_batch_credentials_file(
    platform: &str,
    path: &str,
    content: &str,
) -> Result<Vec<ParsedBatchCredential>, AppError> {
    parse_batch_credentials_text(platform, content).map_err(|error| match error {
        AppError::Validation {
            code,
            message,
            details,
            recoverable,
        } => AppError::Validation {
            code,
            message: format!("{path}: {message}"),
            details,
            recoverable,
        },
        other => other,
    })
}

async fn create_batch_credential(
    pool: &SqlitePool,
    platform: &str,
    batch_id: Option<String>,
    credential: ParsedBatchCredential,
) -> Result<RouteCredential, AppError> {
    let (kind, display_name, email, secret_payload_json, config_json, preview_json) =
        match credential {
            ParsedBatchCredential::Official(credential) => {
                let preview_json = RoutePreviewService::generate(
                    platform,
                    "official",
                    &credential.secret_payload_json,
                    &credential.config_json,
                );
                (
                    "official",
                    credential.display_name,
                    credential.email,
                    credential.secret_payload_json,
                    credential.config_json,
                    preview_json,
                )
            }
            ParsedBatchCredential::Api(credential) => (
                "api",
                credential.display_name,
                credential.email,
                credential.secret_payload_json,
                credential.config_json,
                credential.preview_json,
            ),
        };

    RouteCredentialRepository::create(
        pool,
        platform,
        kind,
        &display_name,
        email,
        "ok",
        batch_id,
        &secret_payload_json,
        &config_json,
        &preview_json,
    )
    .await
}

fn is_api_batch_item(item: &Map<String, Value>) -> bool {
    if item
        .get("x-ai-switch")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("kind"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("api"))
    {
        return true;
    }

    ["api-key", "api_key", "api-key-entries", "api_key_entries"]
        .iter()
        .any(|field| item.get(*field).is_some_and(|value| !value.is_null()))
        || item
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|value| {
                matches!(
                    value
                        .trim()
                        .to_ascii_lowercase()
                        .replace([' ', '-'], "_")
                        .as_str(),
                    "api"
                        | "claude_api_key"
                        | "codex_api_key"
                        | "gemini_api_key"
                        | "xai_api_key"
                        | "openai_compatibility"
                )
            })
}

fn normalize_api_batch_item(
    platform: &str,
    item: &Map<String, Value>,
) -> Result<NormalizedImportItem, AppError> {
    let text = Value::Array(vec![Value::Object(item.clone())]).to_string();
    let normalized = normalize_transfer_items(&text, &[])?;
    let result = normalized.into_iter().next().expect("one transfer item");
    let result = match result {
        Ok(item) => Ok(item),
        Err(issue) if issue.code == "transfer.choice_required" => {
            let platform_id = PlatformId::parse(platform)?;
            let interface_format =
                default_api_interface_format(platform_id).ok_or_else(|| AppError::Validation {
                    code: "validation.cpa_api_platform_required",
                    message: "API Key CPA requires an explicit platform and interface format"
                        .to_string(),
                    details: None,
                    recoverable: true,
                })?;
            let choice = TransferPlatformChoice {
                item_index: 0,
                platform: platform.to_string(),
                interface_format: Some(interface_format.to_string()),
            };
            normalize_transfer_items(&text, &[choice])?
                .into_iter()
                .next()
                .expect("one transfer item")
        }
        Err(issue) => Err(issue),
    };
    let item = result.map_err(|issue| AppError::Validation {
        code: "validation.cpa_api_key_invalid",
        message: "API Key CPA credential is invalid".to_string(),
        details: Some(issue.code),
        recoverable: true,
    })?;
    if item.platform != platform {
        return Err(AppError::Validation {
            code: "validation.cpa_platform_mismatch",
            message: "CPA credential type does not match the selected platform".to_string(),
            details: Some(format!("expected {platform}, got {}", item.platform)),
            recoverable: true,
        });
    }
    Ok(item)
}

fn default_api_interface_format(platform: PlatformId) -> Option<&'static str> {
    match platform {
        PlatformId::Codex => Some("openai-responses"),
        PlatformId::Claude => Some("anthropic"),
        PlatformId::Gemini => Some("gemini"),
        PlatformId::Grok => Some("openai"),
        _ => None,
    }
}

fn validate_model_mappings(value: &str) -> Result<(), AppError> {
    let mappings: Vec<ModelMapping> = serde_json::from_str(value)?;
    if mappings
        .iter()
        .any(|mapping| mapping.from.trim().is_empty() || mapping.to.trim().is_empty())
    {
        return Err(AppError::Validation {
            code: "validation.model_mapping",
            message: "Model mappings require from and to".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        });
    }
    if mappings.iter().any(|mapping| {
        mapping.from.trim() == "upstream-model" || mapping.to.trim() == "upstream-model"
    }) {
        return Err(AppError::Validation {
            code: "validation.model_mapping",
            message: "Model mapping uses the upstream-model placeholder".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        });
    }
    Ok(())
}

fn parse_fetched_models_json(value: Option<&str>) -> Result<Vec<FetchedRouteModel>, AppError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut models = serde_json::from_str::<Vec<FetchedRouteModel>>(value).map_err(|err| {
        AppError::Validation {
            code: "validation.fetched_models",
            message: "Fetched models must be a valid JSON array".to_string(),
            details: Some(err.to_string()),
            recoverable: true,
        }
    })?;
    if models.iter().any(|model| model.id.trim().is_empty()) {
        return Err(AppError::Validation {
            code: "validation.fetched_models",
            message: "Fetched models require a non-empty id".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        });
    }
    for model in &mut models {
        model.id = model.id.trim().to_string();
        model.owned_by = model
            .owned_by
            .take()
            .map(|owned_by| owned_by.trim().to_string())
            .filter(|owned_by| !owned_by.is_empty());
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_empty_model_mappings() {
        assert!(validate_model_mappings("[]").is_ok());
    }

    #[test]
    fn validates_route_priority_range() {
        assert_eq!(validate_route_priority(1).unwrap(), 1);
        assert_eq!(validate_route_priority(5).unwrap(), 5);
        assert!(validate_route_priority(0).is_err());
        assert!(validate_route_priority(6).is_err());
    }

    #[test]
    fn validates_max_concurrency_lower_bound() {
        assert_eq!(validate_max_concurrency(1).unwrap(), 1);
        assert!(validate_max_concurrency(0).is_err());
        assert!(validate_max_concurrency(-1).is_err());
    }

    #[test]
    fn rejects_placeholder_model_mapping() {
        let error = validate_model_mappings(r#"[{"from":"gpt-5","to":"upstream-model"}]"#)
            .expect_err("placeholder should be rejected");

        match error {
            AppError::Validation { code, message, .. } => {
                assert_eq!(code, "validation.model_mapping");
                assert!(message.contains("upstream-model"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn validates_anthropic_api_key_field() {
        assert_eq!(
            validate_api_key_field(Some("ANTHROPIC_AUTH_TOKEN"), "anthropic").unwrap(),
            Some("ANTHROPIC_AUTH_TOKEN")
        );
        assert!(validate_api_key_field(Some("ANTHROPIC_AUTH_TOKEN"), "openai").is_err());
        assert!(validate_api_key_field(Some("bad"), "anthropic").is_err());
    }

    #[test]
    fn validate_interface_format_rejects_legacy_anthropic_alias() {
        let legacy = ["anthropic", "messages"].join("-");
        assert!(validate_interface_format("anthropic").is_ok());
        assert!(validate_interface_format(&legacy).is_err());
    }

    #[test]
    fn official_import_parser_accepts_sub2api_k12_codex() {
        let text = r#"{
          "name": "tallisbisaccia737@hotmail.com",
          "type": "oauth",
          "platform": "openai",
          "credentials": {
            "type": "oauth",
            "email": "tallisbisaccia737@hotmail.com",
            "id_token": "id-token",
            "auth_mode": "agentIdentity",
            "plan_type": "k12",
            "account_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "workspace_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "agent_private_key": "private-key"
          }
        }"#;

        let parsed = parse_official_credentials_text("codex", text).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].email.as_deref(),
            Some("tallisbisaccia737@hotmail.com")
        );
        assert!(parsed[0].secret_payload_json.contains("private-key"));
        assert!(parsed[0]
            .config_json
            .contains("\"subscription_type\":\"k12\""));
    }

    #[test]
    fn official_import_parser_falls_back_to_cpa() {
        let text = r#"{"type":"codex","access_token":"at","refresh_token":"rt"}"#;

        let parsed = parse_official_credentials_text("codex", text).unwrap();

        assert_eq!(parsed.len(), 1);
        assert!(parsed[0]
            .secret_payload_json
            .contains("\"access_token\":\"at\""));
        assert!(parsed[0].config_json.contains("\"raw_type\":\"codex\""));
    }

    #[tokio::test]
    async fn official_text_import_accepts_sub2api_through_existing_entry() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let text = r#"{
          "name": "tallisbisaccia737@hotmail.com",
          "type": "oauth",
          "platform": "openai",
          "credentials": {
            "type": "oauth",
            "email": "tallisbisaccia737@hotmail.com",
            "id_token": "id-token",
            "auth_mode": "agentIdentity",
            "plan_type": "k12",
            "account_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "workspace_id": "7fbe4da7-1fab-4f6d-8210-a1eba367f805",
            "agent_private_key": "private-key"
          }
        }"#;

        let result = RouteCredentialService::import_official_text(
            &pool,
            ImportOfficialTextInput {
                platform: "codex".to_string(),
                text: text.to_string(),
                batch_name: Some("Sub2API K12".to_string()),
            },
        )
        .await
        .expect("import");

        assert!(result.failed.is_empty());
        assert_eq!(result.imported.len(), 1);
        let imported = &result.imported[0];
        assert_eq!(imported.platform, "codex");
        assert_eq!(imported.kind, "official");
        assert_eq!(
            imported.email.as_deref(),
            Some("tallisbisaccia737@hotmail.com")
        );
        assert_eq!(imported.subscription_type.as_deref(), Some("k12"));
        assert_eq!(imported.batch_name.as_deref(), Some("Sub2API K12"));
        assert!(imported.secret_payload_json.contains("private-key"));
        assert!(imported
            .config_json
            .contains("\"import_format\":\"sub2api\""));
    }

    #[tokio::test]
    async fn batch_import_accepts_mixed_oauth_and_api_key_cpa_items() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let text = r#"[
          {
            "type": "codex",
            "access_token": "oauth-access",
            "refresh_token": "oauth-refresh"
          },
          {
            "api-key": "sk-exported",
            "base-url": "https://api.example.invalid/v1",
            "x-ai-switch": {
              "format": "ai-switch.route-credential",
              "schema_version": 1,
              "platform": "codex",
              "kind": "api",
              "cpa_section": "codex-api-key",
              "display_name": "Exported API",
              "interface_format": "openai-responses"
            }
          },
          {
            "type": "codex-api-key",
            "api_key": "sk-legacy",
            "base_url": "https://api.example.invalid/v1"
          }
        ]"#;

        let result = RouteCredentialService::import_official_text(
            &pool,
            ImportOfficialTextInput {
                platform: "codex".to_string(),
                text: text.to_string(),
                batch_name: Some("Mixed CPA".to_string()),
            },
        )
        .await
        .expect("import");

        assert!(result.failed.is_empty());
        assert_eq!(result.imported.len(), 3);
        assert_eq!(
            result
                .imported
                .iter()
                .map(|credential| credential.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["official", "api", "api"]
        );
        assert_eq!(result.imported[1].display_name, "Exported API");
        assert!(result.imported[1]
            .secret_payload_json
            .contains("sk-exported"));
        assert!(result.imported[2].secret_payload_json.contains("sk-legacy"));
        assert!(result.imported[1].config_json.contains("openai-responses"));
        assert!(result
            .imported
            .iter()
            .all(|credential| credential.batch_name.as_deref() == Some("Mixed CPA")));
    }

    #[tokio::test]
    async fn hermes_official_import_is_rejected_before_creating_a_batch() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let error = RouteCredentialService::import_official_text(
            &pool,
            ImportOfficialTextInput {
                platform: "hermes".to_string(),
                text: "{}".to_string(),
                batch_name: Some("Hermes import".to_string()),
            },
        )
        .await
        .expect_err("Hermes official import is unavailable");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "capability.unavailable",
                ..
            }
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM batches")
            .fetch_one(&pool)
            .await
            .expect("batch count");
        assert_eq!(count, 0);
    }

    pub async fn page(
        pool: &SqlitePool,
        request: RouteCredentialPageRequest,
    ) -> Result<RouteCredentialPage, AppError> {
        let platform = PlatformId::parse(&request.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        RouteCredentialRepository::page(
            pool,
            RouteCredentialPageRequest {
                platform: platform.as_str().to_string(),
                ..request
            },
        )
        .await
    }

    pub async fn reorder(
        pool: &SqlitePool,
        input: ReorderRouteCredentialInput,
    ) -> Result<RouteCredentialPage, AppError> {
        let platform = PlatformId::parse(&input.platform)?;
        PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;
        RouteCredentialRepository::reorder(
            pool,
            ReorderRouteCredentialInput {
                platform: platform.as_str().to_string(),
                ..input
            },
        )
        .await
    }

    #[tokio::test]
    async fn create_api_credential_persists_fetched_models() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Cached models".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: Some(
                    r#"[{"id":" gpt-5 ","owned_by":" openai ","supports_1m":true}]"#.into(),
                ),
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
        assert_eq!(
            config["fetched_models"],
            serde_json::json!([{
                "id": "gpt-5",
                "owned_by": "openai",
                "supports_1m": true
            }])
        );
    }

    #[tokio::test]
    async fn create_api_credential_rejects_invalid_fetched_models() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let error = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Invalid cache".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: Some(r#"[{"id":"   "}]"#.into()),
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect_err("invalid cache must fail");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.fetched_models",
                ..
            }
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM route_credentials")
            .fetch_one(&pool)
            .await
            .expect("credential count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn copy_route_credential_appends_date_to_display_name() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Team Account".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: Some(r#"[{"id":"gpt-5","owned_by":"openai"}]"#.into()),
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let copied = RouteCredentialService::copy(&pool, created.id.clone())
            .await
            .expect("copy");

        assert_ne!(copied.id, created.id);
        assert_eq!(copied.platform, created.platform);
        assert_eq!(copied.kind, created.kind);
        assert_eq!(copied.secret_payload_json, created.secret_payload_json);
        assert_eq!(copied.config_json, created.config_json);
        assert!(
            copied.display_name.starts_with("Team Account "),
            "unexpected display name: {}",
            copied.display_name
        );
        assert_eq!(copied.display_name.len(), "Team Account YYYY-MM-DD".len());
    }

    #[tokio::test]
    async fn copy_api_credential_within_platform_without_options_preserves_legacy_payloads() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Legacy API",
            None,
            "ok",
            None,
            r#"{"key":"legacy-secret"}"#,
            r#"not-json"#,
            r#"legacy-preview"#,
        )
        .await
        .expect("create legacy credential");

        let copied = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput::default(),
        )
        .await
        .expect("legacy copy");

        assert_eq!(copied.secret_payload_json, "{\"key\":\"legacy-secret\"}");
        assert_eq!(copied.config_json, "not-json");
        assert_eq!(copied.preview_json, "legacy-preview");
    }

    #[tokio::test]
    async fn copy_api_credential_to_claude_converts_compatible_fields() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Cross-platform API".into(),
                api_key: "sk-source".into(),
                base_url: "https://api.example.com/v1/".into(),
                interface_format: "openai".into(),
                model_mappings_json: r#"[{"from":"gpt-5","to":"vendor-gpt-5"}]"#.into(),
                fetched_models_json: Some(r#"[{"id":"gpt-5"}]"#.into()),
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: Some(true),
                user_agent: Some("shared-client/1.0".into()),
            },
        )
        .await
        .expect("create");
        let mut source_config: Value =
            serde_json::from_str(&source.config_json).expect("source config");
        source_config["failure_policy"] = json!({
            "retry_count": 4,
            "retry_interval_ms": 500,
            "semantic_error_threshold": 20,
        });
        source_config["recovery"] = json!({
            "mode": "scheduled",
            "times": ["08:00"],
        });
        source_config["turn_reminder"] = json!(true);
        sqlx::query("UPDATE route_credentials SET config_json = ? WHERE id = ?")
            .bind(source_config.to_string())
            .bind(&source.id)
            .execute(&pool)
            .await
            .expect("seed source config");
        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&source.id))
            .await
            .expect("seed source pool");

        let copied = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput {
                target_platform: Some("claude".into()),
                api_key: Some("sk-override".into()),
            },
        )
        .await
        .expect("cross-platform copy");

        assert_eq!(copied.platform, "claude");
        assert_eq!(copied.kind, "api");
        assert_eq!(
            serde_json::from_str::<Value>(&copied.secret_payload_json).expect("copied secret"),
            json!({ "api_key": "sk-override" })
        );
        let config: Value = serde_json::from_str(&copied.config_json).expect("copied config");
        assert_eq!(config["base_url"], "https://api.example.com");
        assert_eq!(config["interface_format"], "anthropic");
        assert_eq!(config["headers"], source_config["headers"]);
        assert_eq!(config["failure_policy"], source_config["failure_policy"]);
        assert_eq!(config["recovery"], source_config["recovery"]);
        assert!(config.get("model_mappings").is_none());
        assert!(config.get("fetched_models").is_none());
        assert!(config.get("responses_custom_tool_compat").is_none());
        assert!(config.get("turn_reminder").is_none());
        assert!(!RoutePoolRepository::list_member_ids(&pool, "claude")
            .await
            .expect("target pool")
            .contains(&copied.id));
    }

    #[tokio::test]
    async fn copy_api_credential_to_codex_adds_v1_and_keeps_original_key() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "claude".into(),
                display_name: "Claude API".into(),
                api_key: "sk-source".into(),
                base_url: "https://api.example.com".into(),
                interface_format: "anthropic".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: Some("ANTHROPIC_AUTH_TOKEN".into()),
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let copied = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput {
                target_platform: Some("codex".into()),
                api_key: Some("   ".into()),
            },
        )
        .await
        .expect("cross-platform copy");

        let secret: Value =
            serde_json::from_str(&copied.secret_payload_json).expect("copied secret");
        let config: Value = serde_json::from_str(&copied.config_json).expect("copied config");
        assert_eq!(secret["api_key"], "sk-source");
        assert_eq!(config["base_url"], "https://api.example.com/v1");
        assert_eq!(config["interface_format"], "openai");
        assert!(config.get("api_key_field").is_none());
    }

    #[tokio::test]
    async fn copy_api_credential_within_platform_can_override_api_key() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Same-platform API".into(),
                api_key: "sk-source".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: r#"[{"from":"gpt-5","to":"vendor-gpt-5"}]"#.into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let copied = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput {
                target_platform: Some("codex".into()),
                api_key: Some("sk-override".into()),
            },
        )
        .await
        .expect("same-platform copy");

        let secret: Value =
            serde_json::from_str(&copied.secret_payload_json).expect("copied secret");
        assert_eq!(secret["api_key"], "sk-override");
        assert_eq!(copied.config_json, source.config_json);
        assert!(copied.preview_json.contains("sk-override"));
    }

    #[tokio::test]
    async fn copy_route_credential_preserves_compatible_routing_settings() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Configured API".into(),
                api_key: "sk-source".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");
        sqlx::query(
            "UPDATE route_credentials
             SET route_priority = ?, max_concurrency = ?
             WHERE id = ?",
        )
        .bind(1_i64)
        .bind(7_i64)
        .bind(&source.id)
        .execute(&pool)
        .await
        .expect("configure routing settings");

        let copied = RouteCredentialService::copy(&pool, source.id)
            .await
            .expect("copy");

        assert_eq!(copied.route_priority, 1);
        assert_eq!(copied.max_concurrency, 7);
    }

    #[tokio::test]
    async fn copy_api_credential_to_platform_without_default_dialect_keeps_source_shape() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Custom API".into(),
                api_key: "sk-source".into(),
                base_url: "https://api.example.com/custom".into(),
                interface_format: "openai-responses".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: Some(true),
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let copied = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput {
                target_platform: Some("opencode".into()),
                api_key: None,
            },
        )
        .await
        .expect("cross-platform copy");

        let config: Value = serde_json::from_str(&copied.config_json).expect("copied config");
        assert_eq!(config["base_url"], "https://api.example.com/custom");
        assert_eq!(config["interface_format"], "openai-responses");
    }

    #[tokio::test]
    async fn copy_official_credential_to_another_platform_is_rejected() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let source = RouteCredentialRepository::create(
            &pool,
            "codex",
            "official",
            "Official Account",
            Some("team@example.com".into()),
            "ok",
            None,
            r#"{"access_token":"at"}"#,
            r#"{"type":"codex"}"#,
            "{}",
        )
        .await
        .expect("create official");

        let error = RouteCredentialService::copy_with_options(
            &pool,
            source.id,
            CopyRouteCredentialInput {
                target_platform: Some("claude".into()),
                api_key: None,
            },
        )
        .await
        .expect_err("official cross-platform copy must fail");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.official_cross_platform_copy",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn copy_route_credential_inherits_pool_membership() {
        use crate::database::repositories::route_pool_repository::RoutePoolRepository;

        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Pooled Account".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&source.id))
            .await
            .expect("seed pool member");

        let copied = RouteCredentialService::copy(&pool, source.id.clone())
            .await
            .expect("copy");

        let members = RoutePoolRepository::list_member_ids(&pool, "codex")
            .await
            .expect("members");
        assert!(
            members.contains(&copied.id),
            "copy of a pool member should also be in the pool: {members:?}"
        );
    }

    #[tokio::test]
    async fn copy_route_credential_stays_out_of_pool_when_source_is_out() {
        use crate::database::repositories::route_pool_repository::RoutePoolRepository;

        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let source = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Solo Account".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let copied = RouteCredentialService::copy(&pool, source.id.clone())
            .await
            .expect("copy");

        let members = RoutePoolRepository::list_member_ids(&pool, "codex")
            .await
            .expect("members");
        assert!(
            !members.contains(&copied.id),
            "copy of a non-pool source should remain out of the pool: {members:?}"
        );
    }

    #[tokio::test]
    async fn create_api_credential_persists_responses_custom_tool_compat() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Xiaomi Relay".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.xiaomi.example/v1".into(),
                interface_format: "openai-responses".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: Some(true),
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
        assert_eq!(
            config["responses_custom_tool_compat"],
            serde_json::json!(true)
        );
    }

    #[tokio::test]
    async fn create_api_credential_defaults_responses_custom_tool_compat_off() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "Default Relay".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai-responses".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: None,
            },
        )
        .await
        .expect("create");

        let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
        assert_eq!(
            config["responses_custom_tool_compat"],
            serde_json::json!(false)
        );
    }

    #[tokio::test]
    async fn create_api_credential_persists_user_agent_header() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "grok".into(),
                display_name: "Grok UA".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.x.ai/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: Some("  MyGrokClient/9.9.9  ".into()),
            },
        )
        .await
        .expect("create");

        let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
        assert_eq!(
            config["headers"]["User-Agent"],
            serde_json::json!("MyGrokClient/9.9.9")
        );
    }

    #[tokio::test]
    async fn create_api_credential_omits_user_agent_when_empty() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");

        let created = RouteCredentialService::create_api(
            &pool,
            CreateApiRouteCredentialInput {
                platform: "codex".into(),
                display_name: "No UA".into(),
                api_key: "sk-test".into(),
                base_url: "https://api.example.com/v1".into(),
                interface_format: "openai".into(),
                model_mappings_json: "[]".into(),
                fetched_models_json: None,
                api_key_field: None,
                preview_json: None,
                batch_id: None,
                responses_custom_tool_compat: None,
                user_agent: Some("   ".into()),
            },
        )
        .await
        .expect("create");

        let config: serde_json::Value = serde_json::from_str(&created.config_json).expect("config");
        assert!(config.get("headers").is_none());
    }

    #[tokio::test]
    async fn archive_and_restore_batch_preserves_pool_membership() {
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool)
            .await
            .expect("migrations");
        let first = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "First",
            None,
            "ok",
            None,
            r#"{"api_key":"first"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .expect("first");
        let second = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Second",
            None,
            "ok",
            None,
            r#"{"api_key":"second"}"#,
            r#"{"base_url":"https://example.com","interface_format":"openai","model_mappings":[]}"#,
            "{}",
        )
        .await
        .expect("second");
        crate::database::repositories::route_pool_repository::RoutePoolRepository::replace_members(
            &pool,
            "codex",
            std::slice::from_ref(&first.id),
        )
        .await
        .expect("pool membership");

        RouteCredentialService::archive(&pool, vec![first.id.clone(), second.id.clone()])
            .await
            .expect("archive");
        assert!(RouteCredentialRepository::get(&pool, &first.id)
            .await
            .expect("first archived")
            .archived_at
            .is_some());
        assert_eq!(
            crate::database::repositories::route_pool_repository::RoutePoolRepository::list_member_ids(
                &pool,
                "codex",
            )
            .await
            .expect("members"),
            vec![first.id.clone()]
        );

        RouteCredentialService::restore(&pool, vec![first.id.clone(), second.id.clone()])
            .await
            .expect("restore");
        assert!(RouteCredentialRepository::get(&pool, &first.id)
            .await
            .expect("first restored")
            .archived_at
            .is_none());
    }
}
