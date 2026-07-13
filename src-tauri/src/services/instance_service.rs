use crate::database::repositories::instance_repository::InstanceRepository;
use crate::error::AppError;
use crate::models::instance::{ManagedInstance, NewManagedInstance, SetInstanceStatusRequest};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct InstanceService;

impl InstanceService {
    pub async fn list_instances(pool: &SqlitePool) -> Result<Vec<ManagedInstance>, AppError> {
        InstanceRepository::list(pool).await
    }

    pub async fn create_instance(
        pool: &SqlitePool,
        input: NewManagedInstance,
    ) -> Result<ManagedInstance, AppError> {
        let normalized = normalize_instance(input)?;
        InstanceRepository::create(pool, normalized).await
    }

    pub async fn set_instance_status(
        pool: &SqlitePool,
        request: SetInstanceStatusRequest,
    ) -> Result<ManagedInstance, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.instance_id_required",
                message: "Managed instance id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let status = normalize_status(&request.status)?;
        InstanceRepository::set_status(pool, id, &status).await
    }
}

fn normalize_instance(input: NewManagedInstance) -> Result<NewManagedInstance, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.instance_name_required",
            message: "Managed instance name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let launch_args_json = normalize_string_array_json(&input.launch_args_json)?;
    let env_json = normalize_env_json(&input.env_json)?;
    let profile_json = normalize_object_json(
        &input.profile_json,
        "validation.instance_profile_json",
        "validation.instance_profile_object",
        "Managed instance profile JSON",
    )?;
    let status = normalize_status(&input.status)?;

    Ok(NewManagedInstance {
        name,
        target_app_id: trim_optional(input.target_app_id),
        provider_id: trim_optional(input.provider_id),
        launch_args_json,
        env_json,
        profile_json,
        status,
        notes: trim_optional(input.notes),
    })
}

fn normalize_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "configured" | "running" | "stopped" | "error"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.instance_status",
        message: "Managed instance status must be configured, running, stopped, or error"
            .to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_string_array_json(json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(json, "[]", "validation.instance_launch_args_json")?;
    let Some(values) = value.as_array() else {
        return Err(AppError::Validation {
            code: "validation.instance_launch_args_array",
            message: "Managed instance launch args JSON must be an array".to_string(),
            details: None,
            recoverable: true,
        });
    };

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(AppError::Validation {
                code: "validation.instance_launch_arg_string",
                message: "Managed instance launch args must be strings".to_string(),
                details: None,
                recoverable: true,
            });
        };
        normalized.push(value.trim().to_string());
    }

    serde_json::to_string(&normalized).map_err(AppError::from)
}

fn normalize_env_json(json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(json, "{}", "validation.instance_env_json")?;
    let Some(env) = value.as_object() else {
        return Err(AppError::Validation {
            code: "validation.instance_env_object",
            message: "Managed instance environment JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };

    for (key, value) in env {
        let Some(value) = value.as_str() else {
            return Err(AppError::Validation {
                code: "validation.instance_env_string_values",
                message: "Managed instance environment values must be strings".to_string(),
                details: Some(key.clone()),
                recoverable: true,
            });
        };

        if is_sensitive_key(key) && !is_secret_reference(value) {
            return Err(AppError::Validation {
                code: "validation.instance_env_secret_ref_required",
                message: "Sensitive managed instance environment values must use env:// or secret:// references"
                    .to_string(),
                details: Some(key.clone()),
                recoverable: true,
            });
        }
    }

    serde_json::to_string(&value).map_err(AppError::from)
}

fn normalize_object_json(
    json: &str,
    json_code: &'static str,
    object_code: &'static str,
    label: &str,
) -> Result<String, AppError> {
    let value = parse_json_or_default(json, "{}", json_code)?;
    if !value.is_object() {
        return Err(AppError::Validation {
            code: object_code,
            message: format!("{label} must be an object"),
            details: None,
            recoverable: true,
        });
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
        message: "Managed instance JSON field is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| non_empty_string(value.trim().to_string()))
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn create_instance_normalizes_json() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let instance = InstanceService::create_instance(
            &pool,
            NewManagedInstance {
                name: " Codex Review ".to_string(),
                target_app_id: None,
                provider_id: None,
                launch_args_json: "[\" --profile \",\"review\"]".to_string(),
                env_json: "{\"API_KEY\":\"env://API_KEY\"}".to_string(),
                profile_json: "{\"workspace\":\"review\"}".to_string(),
                status: "CONFIGURED".to_string(),
                notes: Some(" Local ".to_string()),
            },
        )
        .await
        .expect("instance");

        assert_eq!(instance.name, "Codex Review");
        assert_eq!(instance.launch_args_json, "[\"--profile\",\"review\"]");
        assert_eq!(instance.status, "configured");
        assert_eq!(instance.notes.as_deref(), Some("Local"));
    }

    #[tokio::test]
    async fn create_instance_rejects_raw_secret_env() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = InstanceService::create_instance(
            &pool,
            NewManagedInstance {
                name: "Unsafe".to_string(),
                target_app_id: None,
                provider_id: None,
                launch_args_json: "[]".to_string(),
                env_json: "{\"API_KEY\":\"raw-secret\"}".to_string(),
                profile_json: "{}".to_string(),
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.instance_env_secret_ref_required");
    }
}
