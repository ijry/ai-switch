use crate::database::repositories::automation_repository::AutomationRepository;
use crate::error::AppError;
use crate::models::automation::{
    BulkOperation, ItemTag, NewBulkOperation, NewItemTag, NewPluginLink, NewTagRecord, PluginLink,
    SetPluginLinkEnabledRequest, TagRecord,
};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct AutomationService;

impl AutomationService {
    pub async fn list_tags(pool: &SqlitePool) -> Result<Vec<TagRecord>, AppError> {
        AutomationRepository::list_tags(pool).await
    }

    pub async fn create_tag(pool: &SqlitePool, input: NewTagRecord) -> Result<TagRecord, AppError> {
        let normalized = normalize_tag(input)?;
        AutomationRepository::create_tag(pool, normalized).await
    }

    pub async fn list_item_tags(pool: &SqlitePool) -> Result<Vec<ItemTag>, AppError> {
        AutomationRepository::list_item_tags(pool).await
    }

    pub async fn create_item_tag(
        pool: &SqlitePool,
        input: NewItemTag,
    ) -> Result<ItemTag, AppError> {
        let normalized = normalize_item_tag(input)?;
        AutomationRepository::create_item_tag(pool, normalized).await
    }

    pub async fn list_plugin_links(pool: &SqlitePool) -> Result<Vec<PluginLink>, AppError> {
        AutomationRepository::list_plugin_links(pool).await
    }

    pub async fn create_plugin_link(
        pool: &SqlitePool,
        input: NewPluginLink,
    ) -> Result<PluginLink, AppError> {
        let normalized = normalize_plugin_link(input)?;
        AutomationRepository::create_plugin_link(pool, normalized).await
    }

    pub async fn set_plugin_link_enabled(
        pool: &SqlitePool,
        request: SetPluginLinkEnabledRequest,
    ) -> Result<PluginLink, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.plugin_link_id_required",
                message: "Plugin link id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        AutomationRepository::set_plugin_link_enabled(pool, id, request.enabled).await
    }

    pub async fn list_bulk_operations(pool: &SqlitePool) -> Result<Vec<BulkOperation>, AppError> {
        AutomationRepository::list_bulk_operations(pool).await
    }

    pub async fn create_bulk_operation(
        pool: &SqlitePool,
        input: NewBulkOperation,
    ) -> Result<BulkOperation, AppError> {
        let normalized = normalize_bulk_operation(input)?;
        AutomationRepository::create_bulk_operation(pool, normalized).await
    }
}

fn normalize_tag(input: NewTagRecord) -> Result<NewTagRecord, AppError> {
    let name = input.name.trim().to_lowercase();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.tag_name_required",
            message: "Tag name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(AppError::Validation {
            code: "validation.tag_name",
            message: "Tag name may only contain letters, numbers, hyphen, underscore, or dot"
                .to_string(),
            details: Some(input.name),
            recoverable: true,
        });
    }

    let color = trim_optional(input.color);
    if let Some(color) = color.as_deref() {
        if !is_hex_color(color) {
            return Err(AppError::Validation {
                code: "validation.tag_color",
                message: "Tag color must be a hex color such as #3f6f5f".to_string(),
                details: Some(color.to_string()),
                recoverable: true,
            });
        }
    }

    Ok(NewTagRecord {
        name,
        color,
        description: trim_optional(input.description),
    })
}

fn normalize_item_tag(input: NewItemTag) -> Result<NewItemTag, AppError> {
    let tag_id = require_non_empty(input.tag_id, "validation.item_tag_tag_required", "Tag id")?;
    let item_type = normalize_item_type(&input.item_type)?;
    let item_id = require_non_empty(
        input.item_id,
        "validation.item_tag_item_required",
        "Item id",
    )?;

    Ok(NewItemTag {
        tag_id,
        item_type,
        item_id,
    })
}

fn normalize_plugin_link(input: NewPluginLink) -> Result<NewPluginLink, AppError> {
    let name = require_non_empty(
        input.name,
        "validation.plugin_link_name_required",
        "Plugin link name",
    )?;
    let plugin_key = input.plugin_key.trim().to_lowercase();
    if plugin_key.is_empty() {
        return Err(AppError::Validation {
            code: "validation.plugin_key_required",
            message: "Plugin key is required".to_string(),
            details: None,
            recoverable: true,
        });
    }
    if !plugin_key
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(AppError::Validation {
            code: "validation.plugin_key",
            message: "Plugin key may only contain letters, numbers, hyphen, underscore, or dot"
                .to_string(),
            details: Some(input.plugin_key),
            recoverable: true,
        });
    }

    let item_type = normalize_item_type(&input.item_type)?;
    let item_id = require_non_empty(
        input.item_id,
        "validation.plugin_link_item_required",
        "Plugin link item id",
    )?;
    let config_json = normalize_object_json(
        &input.config_json,
        "validation.plugin_link_config_json",
        "validation.plugin_link_config_object",
        "Plugin link config JSON",
    )?;
    let status = normalize_plugin_status(&input.status)?;
    let status = if input.enabled {
        status
    } else {
        "paused".to_string()
    };

    Ok(NewPluginLink {
        name,
        plugin_key,
        item_type,
        item_id,
        config_json,
        enabled: input.enabled,
        status,
        notes: trim_optional(input.notes),
    })
}

fn normalize_bulk_operation(input: NewBulkOperation) -> Result<NewBulkOperation, AppError> {
    let name = require_non_empty(
        input.name,
        "validation.bulk_operation_name_required",
        "Bulk operation name",
    )?;
    let operation_type = normalize_operation_type(&input.operation_type)?;
    let target_type = normalize_item_type(&input.target_type)?;
    let item_ids_json = normalize_string_array_json(&input.item_ids_json)?;
    let parameters_json = normalize_object_json(
        &input.parameters_json,
        "validation.bulk_parameters_json",
        "validation.bulk_parameters_object",
        "Bulk operation parameters JSON",
    )?;
    let summary_json = normalize_object_json(
        &input.summary_json,
        "validation.bulk_summary_json",
        "validation.bulk_summary_object",
        "Bulk operation summary JSON",
    )?;
    let status = normalize_bulk_status(&input.status)?;

    Ok(NewBulkOperation {
        name,
        operation_type,
        target_type,
        item_ids_json,
        parameters_json,
        dry_run: input.dry_run,
        status,
        summary_json,
    })
}

fn normalize_item_type(item_type: &str) -> Result<String, AppError> {
    let normalized = item_type.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "provider"
            | "official_account"
            | "mcp_server"
            | "prompt_asset"
            | "session"
            | "managed_instance"
            | "wakeup_task"
            | "target_app"
            | "mixed"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.automation_item_type",
        message: "Automation item type is not supported".to_string(),
        details: Some(item_type.to_string()),
        recoverable: true,
    })
}

fn normalize_operation_type(operation_type: &str) -> Result<String, AppError> {
    let normalized = operation_type.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "tag_apply" | "tag_remove" | "status_record" | "export_selection" | "plugin_link"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.bulk_operation_type",
        message: "Bulk operation type is not supported".to_string(),
        details: Some(operation_type.to_string()),
        recoverable: true,
    })
}

fn normalize_plugin_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(normalized.as_str(), "configured" | "paused" | "error") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.plugin_link_status",
        message: "Plugin link status must be configured, paused, or error".to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_bulk_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "planned" | "recorded" | "cancelled" | "error"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.bulk_operation_status",
        message: "Bulk operation status must be planned, recorded, cancelled, or error".to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_string_array_json(json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(json, "[]", "validation.bulk_item_ids_json")?;
    let Some(values) = value.as_array() else {
        return Err(AppError::Validation {
            code: "validation.bulk_item_ids_array",
            message: "Bulk operation item IDs JSON must be an array".to_string(),
            details: None,
            recoverable: true,
        });
    };

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(AppError::Validation {
                code: "validation.bulk_item_id_string",
                message: "Bulk operation item IDs must be strings".to_string(),
                details: None,
                recoverable: true,
            });
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(AppError::Validation {
                code: "validation.bulk_item_id_required",
                message: "Bulk operation item IDs cannot contain empty values".to_string(),
                details: None,
                recoverable: true,
            });
        }
        normalized.push(value.to_string());
    }

    serde_json::to_string(&normalized).map_err(AppError::from)
}

fn normalize_object_json(
    json: &str,
    json_code: &'static str,
    object_code: &'static str,
    label: &str,
) -> Result<String, AppError> {
    let value = parse_json_or_default(json, "{}", json_code)?;
    let Some(object) = value.as_object() else {
        return Err(AppError::Validation {
            code: object_code,
            message: format!("{label} must be an object"),
            details: None,
            recoverable: true,
        });
    };

    for (key, value) in object {
        if is_sensitive_key(key) {
            let Some(value) = value.as_str() else {
                return Err(AppError::Validation {
                    code: "validation.automation_secret_ref_required",
                    message:
                        "Sensitive automation metadata must use env:// or secret:// references"
                            .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            };
            if !is_secret_reference(value) {
                return Err(AppError::Validation {
                    code: "validation.automation_secret_ref_required",
                    message:
                        "Sensitive automation metadata must use env:// or secret:// references"
                            .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            }
        }
    }

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
        message: "Automation JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn require_non_empty(value: String, code: &'static str, label: &str) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::Validation {
            code,
            message: format!("{label} is required"),
            details: None,
            recoverable: true,
        });
    }
    Ok(value)
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|character| character.is_ascii_hexdigit())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_tag_normalizes_name_and_color() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let tag = AutomationService::create_tag(
            &pool,
            NewTagRecord {
                name: " Review ".to_string(),
                color: Some("#3F6F5F".to_string()),
                description: Some(" Shared ".to_string()),
            },
        )
        .await
        .expect("tag");

        assert_eq!(tag.name, "review");
        assert_eq!(tag.color.as_deref(), Some("#3F6F5F"));
        assert_eq!(tag.description.as_deref(), Some("Shared"));
    }

    #[tokio::test]
    async fn create_plugin_link_rejects_raw_secret_config() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = AutomationService::create_plugin_link(
            &pool,
            NewPluginLink {
                name: "Unsafe".to_string(),
                plugin_key: "unsafe.plugin".to_string(),
                item_type: "provider".to_string(),
                item_id: "provider-1".to_string(),
                config_json: "{\"api_key\":\"raw-secret\"}".to_string(),
                enabled: true,
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.automation_secret_ref_required");
    }

    #[tokio::test]
    async fn create_bulk_operation_rejects_non_string_item_ids() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = AutomationService::create_bulk_operation(
            &pool,
            NewBulkOperation {
                name: "Broken bulk".to_string(),
                operation_type: "tag_apply".to_string(),
                target_type: "provider".to_string(),
                item_ids_json: "[123]".to_string(),
                parameters_json: "{}".to_string(),
                dry_run: true,
                status: "planned".to_string(),
                summary_json: "{}".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.bulk_item_id_string");
    }
}
