use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::error::AppError;
use crate::models::batch::NewBatch;
use crate::models::route_credential::{
    normalize_anthropic_api_key_field, CreateApiRouteCredentialInput, ImportOfficialFilesInput,
    ImportOfficialTextInput, ModelMapping, RouteCredential, RouteCredentialImportFailure,
    RouteCredentialImportResult, UpdateRouteCredentialInput,
};
use crate::services::cpa_import_service::{parse_cpa_file, parse_cpa_text};
use crate::services::route_preview_service::RoutePreviewService;
use serde_json::json;
use sqlx::SqlitePool;

pub struct RouteCredentialService;

impl RouteCredentialService {
    pub async fn list(
        pool: &SqlitePool,
        platform: String,
    ) -> Result<Vec<RouteCredential>, AppError> {
        RouteCredentialRepository::list_by_platform(pool, &normalize_platform(&platform)?).await
    }

    pub async fn get(pool: &SqlitePool, id: String) -> Result<RouteCredential, AppError> {
        RouteCredentialRepository::get(pool, &id).await
    }

    pub async fn create_api(
        pool: &SqlitePool,
        input: CreateApiRouteCredentialInput,
    ) -> Result<RouteCredential, AppError> {
        let platform = normalize_platform(&input.platform)?;
        validate_required("display_name", &input.display_name)?;
        validate_required("api_key", &input.api_key)?;
        validate_required("base_url", &input.base_url)?;
        validate_interface_format(&input.interface_format)?;
        validate_model_mappings(&input.model_mappings_json)?;
        let api_key_field =
            validate_api_key_field(input.api_key_field.as_deref(), &input.interface_format)?;

        let secret_payload_json = json!({ "api_key": input.api_key.trim() }).to_string();
        let mut config = json!({
            "base_url": input.base_url.trim(),
            "interface_format": input.interface_format,
            "model_mappings": serde_json::from_str::<serde_json::Value>(&input.model_mappings_json)?,
        });
        if let Some(api_key_field) = api_key_field {
            config["api_key_field"] = json!(api_key_field);
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
        let platform = normalize_platform(&input.platform)?;
        let batch_id = ensure_optional_batch(pool, input.batch_name).await?;
        let parsed = parse_cpa_text(&platform, &input.text)?;
        let mut imported = Vec::with_capacity(parsed.len());

        for credential in parsed {
            let preview_json = RoutePreviewService::generate(
                &platform,
                "official",
                &credential.secret_payload_json,
                &credential.config_json,
            );
            imported.push(
                RouteCredentialRepository::create(
                    pool,
                    &platform,
                    "official",
                    &credential.display_name,
                    credential.email,
                    "ok",
                    batch_id.clone(),
                    &credential.secret_payload_json,
                    &credential.config_json,
                    &preview_json,
                )
                .await?,
            );
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
        let platform = normalize_platform(&input.platform)?;
        let batch_id = ensure_optional_batch(pool, input.batch_name).await?;
        let mut imported = Vec::new();
        let mut failed = Vec::new();

        for path in input.file_paths {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match parse_cpa_file(&platform, &path, &content) {
                    Ok(credentials) => {
                        for credential in credentials {
                            let preview_json = RoutePreviewService::generate(
                                &platform,
                                "official",
                                &credential.secret_payload_json,
                                &credential.config_json,
                            );
                            imported.push(
                                RouteCredentialRepository::create(
                                    pool,
                                    &platform,
                                    "official",
                                    &credential.display_name,
                                    credential.email,
                                    "ok",
                                    batch_id.clone(),
                                    &credential.secret_payload_json,
                                    &credential.config_json,
                                    &preview_json,
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
        RouteCredentialRepository::update(pool, &id, &input).await
    }

    pub async fn delete(pool: &SqlitePool, id: String) -> Result<(), AppError> {
        RouteCredentialRepository::delete(pool, &id).await
    }
}

async fn ensure_optional_batch(
    pool: &SqlitePool,
    batch_name: Option<String>,
) -> Result<Option<String>, AppError> {
    let Some(name) = batch_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
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

fn validate_interface_format(value: &str) -> Result<(), AppError> {
    match value {
        "openai" | "openai-responses" | "anthropic" | "anthropic-messages" | "gemini" => Ok(()),
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
    matches!(value, "anthropic" | "anthropic-messages")
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

fn normalize_platform(platform: &str) -> Result<String, AppError> {
    let platform = platform.trim();
    if platform.is_empty() {
        return Err(AppError::Validation {
            code: "validation.platform_required",
            message: "Platform is required".to_string(),
            details: None,
            recoverable: true,
        });
    }
    Ok(platform.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_empty_model_mappings() {
        assert!(validate_model_mappings("[]").is_ok());
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
}
