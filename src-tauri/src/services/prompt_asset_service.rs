use crate::database::repositories::prompt_asset_repository::PromptAssetRepository;
use crate::error::AppError;
use crate::models::prompt_asset::{NewPromptAsset, PromptAsset, SetPromptAssetEnabledRequest};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct PromptAssetService;

impl PromptAssetService {
    pub async fn list_prompt_assets(pool: &SqlitePool) -> Result<Vec<PromptAsset>, AppError> {
        PromptAssetRepository::list(pool).await
    }

    pub async fn create_prompt_asset(
        pool: &SqlitePool,
        input: NewPromptAsset,
    ) -> Result<PromptAsset, AppError> {
        let normalized = normalize_prompt_asset(input)?;
        PromptAssetRepository::create(pool, normalized).await
    }

    pub async fn set_prompt_asset_enabled(
        pool: &SqlitePool,
        request: SetPromptAssetEnabledRequest,
    ) -> Result<PromptAsset, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.prompt_asset_id_required",
                message: "Prompt asset id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        PromptAssetRepository::set_enabled(pool, id, request.enabled).await
    }
}

fn normalize_prompt_asset(input: NewPromptAsset) -> Result<NewPromptAsset, AppError> {
    let item_type = normalize_item_type(&input.item_type)?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.prompt_asset_name_required",
            message: "Prompt or skill name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let body = input.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::Validation {
            code: "validation.prompt_asset_body_required",
            message: "Prompt or skill body is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let description = input
        .description
        .and_then(|description| non_empty_string(description.trim().to_string()));
    let tags_json = normalize_tags_json(&input.tags_json)?;
    let metadata_json = normalize_metadata_json(&input.metadata_json)?;

    Ok(NewPromptAsset {
        item_type,
        name,
        description,
        body,
        tags_json,
        metadata_json,
        enabled: input.enabled,
    })
}

fn normalize_item_type(item_type: &str) -> Result<String, AppError> {
    let normalized = item_type.trim().to_lowercase();
    if matches!(normalized.as_str(), "prompt" | "skill") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.prompt_asset_type",
        message: "Prompt asset type must be prompt or skill".to_string(),
        details: Some(item_type.to_string()),
        recoverable: true,
    })
}

fn normalize_tags_json(tags_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(tags_json, "[]", "validation.prompt_asset_tags_json")?;
    let Some(tags) = value.as_array() else {
        return Err(AppError::Validation {
            code: "validation.prompt_asset_tags_array",
            message: "Prompt asset tags JSON must be an array".to_string(),
            details: None,
            recoverable: true,
        });
    };

    if tags.iter().any(|tag| !tag.is_string()) {
        return Err(AppError::Validation {
            code: "validation.prompt_asset_tags_string_values",
            message: "Prompt asset tags must be strings".to_string(),
            details: None,
            recoverable: true,
        });
    }

    serde_json::to_string(&value).map_err(AppError::from)
}

fn normalize_metadata_json(metadata_json: &str) -> Result<String, AppError> {
    let value =
        parse_json_or_default(metadata_json, "{}", "validation.prompt_asset_metadata_json")?;
    if !value.is_object() {
        return Err(AppError::Validation {
            code: "validation.prompt_asset_metadata_object",
            message: "Prompt asset metadata JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    }

    reject_raw_secret_metadata(&value)?;
    serde_json::to_string(&value).map_err(AppError::from)
}

fn parse_json_or_default(
    json: &str,
    default_json: &str,
    code: &'static str,
) -> Result<Value, AppError> {
    let json = if json.trim().is_empty() {
        default_json
    } else {
        json.trim()
    };

    serde_json::from_str(json).map_err(|error| AppError::Validation {
        code,
        message: "Prompt asset JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn reject_raw_secret_metadata(value: &Value) -> Result<(), AppError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if is_sensitive_key(key) {
                    let Some(secret_ref) = child.as_str() else {
                        return Err(secret_metadata_error(key));
                    };
                    if !is_secret_reference(secret_ref) {
                        return Err(secret_metadata_error(key));
                    }
                }
                reject_raw_secret_metadata(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                reject_raw_secret_metadata(child)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn secret_metadata_error(key: &str) -> AppError {
    AppError::Validation {
        code: "validation.prompt_asset_metadata_secret_ref_required",
        message: "Sensitive prompt asset metadata must use env:// or secret:// references"
            .to_string(),
        details: Some(key.to_string()),
        recoverable: true,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_lowercase();
    ["token", "api_key", "apikey", "password", "secret"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn is_secret_reference(value: &str) -> bool {
    value.starts_with("env://") || value.starts_with("secret://")
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_prompt_asset_normalizes_prompt() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let asset = PromptAssetService::create_prompt_asset(
            &pool,
            NewPromptAsset {
                item_type: "PROMPT".to_string(),
                name: " Review ".to_string(),
                description: Some(" Code review ".to_string()),
                body: " Find regressions. ".to_string(),
                tags_json: "[\"review\",\"quality\"]".to_string(),
                metadata_json: "{\"owner\":\"engineering\"}".to_string(),
                enabled: true,
            },
        )
        .await
        .expect("asset");

        assert_eq!(asset.item_type, "prompt");
        assert_eq!(asset.name, "Review");
        assert_eq!(asset.description.as_deref(), Some("Code review"));
        assert_eq!(asset.body, "Find regressions.");
        assert_eq!(asset.tags_json, "[\"review\",\"quality\"]");
    }

    #[tokio::test]
    async fn create_prompt_asset_rejects_raw_secret_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = PromptAssetService::create_prompt_asset(
            &pool,
            NewPromptAsset {
                item_type: "skill".to_string(),
                name: "Unsafe Skill".to_string(),
                description: None,
                body: "Use a configured provider.".to_string(),
                tags_json: "[]".to_string(),
                metadata_json: "{\"api_key\":\"raw-token\"}".to_string(),
                enabled: true,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(
            error.code(),
            "validation.prompt_asset_metadata_secret_ref_required"
        );
    }

    #[tokio::test]
    async fn create_prompt_asset_rejects_non_string_tags() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = PromptAssetService::create_prompt_asset(
            &pool,
            NewPromptAsset {
                item_type: "prompt".to_string(),
                name: "Bad Tags".to_string(),
                description: None,
                body: "Find regressions.".to_string(),
                tags_json: "[\"review\", 1]".to_string(),
                metadata_json: "{}".to_string(),
                enabled: true,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.prompt_asset_tags_string_values");
    }
}
