use crate::database::repositories::session_repository::SessionRepository;
use crate::error::AppError;
use crate::models::session::{
    ListSessionEventsRequest, NewSessionEvent, NewSessionRecord, SessionEvent, SessionRecord,
    SetSessionStatusRequest,
};
use serde_json::Value;
use sqlx::SqlitePool;

pub struct SessionService;

impl SessionService {
    pub async fn list_sessions(pool: &SqlitePool) -> Result<Vec<SessionRecord>, AppError> {
        SessionRepository::list_sessions(pool).await
    }

    pub async fn create_session(
        pool: &SqlitePool,
        input: NewSessionRecord,
    ) -> Result<SessionRecord, AppError> {
        let normalized = normalize_session(input)?;
        SessionRepository::create_session(pool, normalized).await
    }

    pub async fn set_session_status(
        pool: &SqlitePool,
        request: SetSessionStatusRequest,
    ) -> Result<SessionRecord, AppError> {
        let id = request.id.trim();
        if id.is_empty() {
            return Err(AppError::Validation {
                code: "validation.session_id_required",
                message: "Session id is required".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let status = normalize_status(&request.status)?;
        SessionRepository::set_session_status(pool, id, &status).await
    }

    pub async fn list_session_events(
        pool: &SqlitePool,
        request: ListSessionEventsRequest,
    ) -> Result<Vec<SessionEvent>, AppError> {
        let session_id = request
            .session_id
            .and_then(|id| non_empty_string(id.trim().to_string()));
        SessionRepository::list_events(pool, session_id.as_deref()).await
    }

    pub async fn create_session_event(
        pool: &SqlitePool,
        input: NewSessionEvent,
    ) -> Result<SessionEvent, AppError> {
        let normalized = normalize_session_event(input)?;
        SessionRepository::create_event(pool, normalized).await
    }
}

fn normalize_session(input: NewSessionRecord) -> Result<NewSessionRecord, AppError> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::Validation {
            code: "validation.session_title_required",
            message: "Session title is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let status = normalize_status(&input.status)?;
    let mcp_server_ids_json = normalize_string_array_json(
        &input.mcp_server_ids_json,
        "[]",
        "validation.session_mcp_ids_json",
        "validation.session_mcp_ids_array",
        "validation.session_mcp_id_string",
        "validation.session_mcp_id_required",
        "Session MCP server IDs",
    )?;
    let tags_json = normalize_string_array_json(
        &input.tags_json,
        "[]",
        "validation.session_tags_json",
        "validation.session_tags_array",
        "validation.session_tag_string",
        "validation.session_tag_required",
        "Session tags",
    )?;

    Ok(NewSessionRecord {
        title,
        target_app_id: trim_optional(input.target_app_id),
        provider_id: trim_optional(input.provider_id),
        official_account_id: trim_optional(input.official_account_id),
        prompt_asset_id: trim_optional(input.prompt_asset_id),
        mcp_server_ids_json,
        tags_json,
        status,
        notes: trim_optional(input.notes),
    })
}

fn normalize_session_event(input: NewSessionEvent) -> Result<NewSessionEvent, AppError> {
    let session_id = input.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(AppError::Validation {
            code: "validation.session_event_session_required",
            message: "Session event requires a session id".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let event_type = input.event_type.trim().to_lowercase();
    if !matches!(
        event_type.as_str(),
        "note" | "status" | "usage" | "quota" | "error" | "import" | "switch"
    ) {
        return Err(AppError::Validation {
            code: "validation.session_event_type",
            message: "Session event type is not supported".to_string(),
            details: Some(input.event_type),
            recoverable: true,
        });
    }

    let message = input.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::Validation {
            code: "validation.session_event_message_required",
            message: "Session event message is required".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let metadata_json = normalize_metadata_json(&input.metadata_json)?;

    Ok(NewSessionEvent {
        session_id,
        event_type,
        message,
        metadata_json,
    })
}

fn normalize_status(status: &str) -> Result<String, AppError> {
    let normalized = status.trim().to_lowercase();
    if matches!(normalized.as_str(), "draft" | "active" | "archived") {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.session_status",
        message: "Session status must be draft, active, or archived".to_string(),
        details: Some(status.to_string()),
        recoverable: true,
    })
}

fn normalize_string_array_json(
    json: &str,
    default_json: &str,
    json_code: &'static str,
    array_code: &'static str,
    string_code: &'static str,
    required_code: &'static str,
    label: &str,
) -> Result<String, AppError> {
    let value = parse_json_or_default(json, default_json, json_code)?;
    let Some(values) = value.as_array() else {
        return Err(AppError::Validation {
            code: array_code,
            message: format!("{label} JSON must be an array"),
            details: None,
            recoverable: true,
        });
    };

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(AppError::Validation {
                code: string_code,
                message: format!("{label} must be strings"),
                details: None,
                recoverable: true,
            });
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(AppError::Validation {
                code: required_code,
                message: format!("{label} cannot contain empty values"),
                details: None,
                recoverable: true,
            });
        }
        normalized.push(value.to_string());
    }

    serde_json::to_string(&normalized).map_err(AppError::from)
}

fn normalize_metadata_json(metadata_json: &str) -> Result<String, AppError> {
    let value = parse_json_or_default(
        metadata_json,
        "{}",
        "validation.session_event_metadata_json",
    )?;
    let Some(metadata) = value.as_object() else {
        return Err(AppError::Validation {
            code: "validation.session_event_metadata_object",
            message: "Session event metadata JSON must be an object".to_string(),
            details: None,
            recoverable: true,
        });
    };

    for (key, value) in metadata {
        if is_sensitive_key(key) {
            let Some(value) = value.as_str() else {
                return Err(AppError::Validation {
                    code: "validation.session_event_metadata_secret_ref_required",
                    message:
                        "Sensitive session event metadata must use env:// or secret:// references"
                            .to_string(),
                    details: Some(key.clone()),
                    recoverable: true,
                });
            };
            if !is_secret_reference(value) {
                return Err(AppError::Validation {
                    code: "validation.session_event_metadata_secret_ref_required",
                    message:
                        "Sensitive session event metadata must use env:// or secret:// references"
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
        message: "Session JSON field is invalid".to_string(),
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
    async fn create_session_normalizes_arrays_and_status() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let session = SessionService::create_session(
            &pool,
            NewSessionRecord {
                title: " Release review ".to_string(),
                target_app_id: None,
                provider_id: None,
                official_account_id: None,
                prompt_asset_id: None,
                mcp_server_ids_json: "[\" mcp-1 \"]".to_string(),
                tags_json: "[\" review \"]".to_string(),
                status: "DRAFT".to_string(),
                notes: Some(" Notes ".to_string()),
            },
        )
        .await
        .expect("session");

        assert_eq!(session.title, "Release review");
        assert_eq!(session.status, "draft");
        assert_eq!(session.mcp_server_ids_json, "[\"mcp-1\"]");
        assert_eq!(session.tags_json, "[\"review\"]");
        assert_eq!(session.notes.as_deref(), Some("Notes"));
    }

    #[tokio::test]
    async fn create_session_rejects_non_string_tags() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = SessionService::create_session(
            &pool,
            NewSessionRecord {
                title: "Broken".to_string(),
                target_app_id: None,
                provider_id: None,
                official_account_id: None,
                prompt_asset_id: None,
                mcp_server_ids_json: "[]".to_string(),
                tags_json: "[123]".to_string(),
                status: "draft".to_string(),
                notes: None,
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.session_tag_string");
    }

    #[tokio::test]
    async fn create_session_event_rejects_raw_secret_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let session = SessionService::create_session(
            &pool,
            NewSessionRecord {
                title: "Session".to_string(),
                target_app_id: None,
                provider_id: None,
                official_account_id: None,
                prompt_asset_id: None,
                mcp_server_ids_json: "[]".to_string(),
                tags_json: "[]".to_string(),
                status: "draft".to_string(),
                notes: None,
            },
        )
        .await
        .expect("session");

        let error = SessionService::create_session_event(
            &pool,
            NewSessionEvent {
                session_id: session.id,
                event_type: "note".to_string(),
                message: "Unsafe".to_string(),
                metadata_json: "{\"api_key\":\"raw-secret\"}".to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(
            error.code(),
            "validation.session_event_metadata_secret_ref_required"
        );
    }
}
