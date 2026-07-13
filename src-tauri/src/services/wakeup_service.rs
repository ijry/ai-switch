use crate::database::repositories::wakeup_repository::WakeupRepository;
use crate::error::AppError;
use crate::models::wakeup::{
    ListWakeupRunsRequest, NewWakeupRun, NewWakeupTask, SetWakeupTaskEnabledRequest, WakeupRun,
    WakeupTask,
};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct WakeupService;

impl WakeupService {
    pub async fn list_wakeup_tasks(pool: &SqlitePool) -> Result<Vec<WakeupTask>, AppError> {
        WakeupRepository::list_tasks(pool).await
    }

    pub async fn create_wakeup_task(
        pool: &SqlitePool,
        input: NewWakeupTask,
    ) -> Result<WakeupTask, AppError> {
        let normalized = normalize_task(input)?;
        WakeupRepository::create_task(pool, normalized).await
    }

    pub async fn set_wakeup_task_enabled(
        pool: &SqlitePool,
        request: SetWakeupTaskEnabledRequest,
    ) -> Result<WakeupTask, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.wakeup_task_id_required",
                message: "Wakeup task id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }

        WakeupRepository::set_task_enabled(pool, id, request.enabled).await
    }

    pub async fn list_wakeup_runs(
        pool: &SqlitePool,
        request: ListWakeupRunsRequest,
    ) -> Result<Vec<WakeupRun>, AppError> {
        let task_id = request
            .task_id
            .and_then(|id| non_empty_string(id.trim().to_string()));
        WakeupRepository::list_runs(pool, task_id.as_deref()).await
    }

    pub async fn create_wakeup_run(
        pool: &SqlitePool,
        input: NewWakeupRun,
    ) -> Result<WakeupRun, AppError> {
        let normalized = normalize_run(input)?;
        WakeupRepository::create_run(pool, normalized).await
    }
}

fn normalize_task(input: NewWakeupTask) -> Result<NewWakeupTask, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation {
            code: "validation.wakeup_task_name_required",
            message: "Wakeup task name is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let trigger_type = normalize_trigger_type(&input.trigger_type)?;
    let schedule_json = normalize_object_json(
        &input.schedule_json,
        "validation.wakeup_schedule_json",
        "validation.wakeup_schedule_object",
        "Wakeup task schedule JSON",
    )?;
    let action_json = normalize_object_json(
        &input.action_json,
        "validation.wakeup_action_json",
        "validation.wakeup_action_object",
        "Wakeup task action JSON",
    )?;
    let status = normalize_status(&input.status)?;
    let status = if input.enabled {
        status
    } else {
        "paused".to_string()
    };

    Ok(NewWakeupTask {
        name,
        managed_instance_id: trim_optional(input.managed_instance_id),
        target_app_id: trim_optional(input.target_app_id),
        provider_id: trim_optional(input.provider_id),
        trigger_type,
        schedule_json,
        action_json,
        enabled: input.enabled,
        status,
        notes: trim_optional(input.notes),
    })
}

fn normalize_run(input: NewWakeupRun) -> Result<NewWakeupRun, AppError> {
    let task_id = input.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(AppError::Validation {
            code: "validation.wakeup_run_task_required",
            message: "Wakeup run requires a task id".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let outcome = normalize_outcome(&input.outcome)?;
    let message = input.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::Validation {
            code: "validation.wakeup_run_message_required",
            message: "Wakeup run message is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let metadata_json = normalize_object_json(
        &input.metadata_json,
        "validation.wakeup_run_metadata_json",
        "validation.wakeup_run_metadata_object",
        "Wakeup run metadata JSON",
    )?;

    Ok(NewWakeupRun {
        task_id,
        outcome,
        message,
        metadata_json,
    })
}

fn normalize_trigger_type(trigger_type: &str) -> Result<String, AppError> {
    let normalized = trigger_type.trim().to_lowercase();
    if matches!(normalized.as_str(), "manual" | "scheduled" | "interval") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.wakeup_trigger_type",
        message: "Wakeup trigger type must be manual, scheduled, or interval".to_string(),
        details: Some(trigger_type.to_string()),
        recoverable: true,
    })
}

fn normalize_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(normalized.as_str(), "configured" | "paused" | "error") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.wakeup_status",
        message: "Wakeup task status must be configured, paused, or error".to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_outcome(outcome: &str) -> Result<String, AppError> {
    let normalized = outcome.trim().to_lowercase();
    if matches!(normalized.as_str(), "recorded" | "skipped" | "failed") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.wakeup_run_outcome",
        message: "Wakeup run outcome must be recorded, skipped, or failed".to_string(),
        details: Some(outcome.to_string()),
        recoverable: true,
    })
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
                    code: "validation.wakeup_secret_ref_required",
                    message: "Sensitive wakeup fields must use env:// or secret:// references"
                        .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            };
            if !is_secret_reference(value) {
                return Err(AppError::Validation {
                    code: "validation.wakeup_secret_ref_required",
                    message: "Sensitive wakeup fields must use env:// or secret:// references"
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
        message: "Wakeup JSON field is invalid".to_string(),
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
    async fn create_wakeup_task_normalizes_json_and_paused_status() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let task = WakeupService::create_wakeup_task(
            &pool,
            NewWakeupTask {
                name: " Morning review ".to_string(),
                managed_instance_id: None,
                target_app_id: None,
                provider_id: None,
                trigger_type: "MANUAL".to_string(),
                schedule_json: "{\" window \":\"morning\"}".to_string(),
                action_json: "{\"kind\":\"status_record\"}".to_string(),
                enabled: false,
                status: "CONFIGURED".to_string(),
                notes: Some(" Local ".to_string()),
            },
        )
        .await
        .expect("task");

        assert_eq!(task.name, "Morning review");
        assert_eq!(task.trigger_type, "manual");
        assert_eq!(task.status, "paused");
        assert_eq!(task.schedule_json, "{\" window \":\"morning\"}");
        assert_eq!(task.notes.as_deref(), Some("Local"));
    }

    #[tokio::test]
    async fn create_wakeup_task_rejects_raw_secret_action() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = WakeupService::create_wakeup_task(
            &pool,
            NewWakeupTask {
                name: "Unsafe".to_string(),
                managed_instance_id: None,
                target_app_id: None,
                provider_id: None,
                trigger_type: "manual".to_string(),
                schedule_json: "{}".to_string(),
                action_json: "{\"api_key\":\"raw-secret\"}".to_string(),
                enabled: true,
                status: "configured".to_string(),
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.wakeup_secret_ref_required");
    }

    #[tokio::test]
    async fn create_wakeup_run_rejects_array_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = WakeupService::create_wakeup_run(
            &pool,
            NewWakeupRun {
                task_id: "task-1".to_string(),
                outcome: "recorded".to_string(),
                message: "Ready".to_string(),
                metadata_json: "[]".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.wakeup_run_metadata_object");
    }
}
