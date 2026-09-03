use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Validation {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Filesystem {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Database {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
    #[error("{message}")]
    Secret {
        code: &'static str,
        message: String,
        details: Option<String>,
        recoverable: bool,
    },
}

impl AppError {
    /// The stable error code. Mirrors what `ApiError` surfaces, for call sites
    /// that need the code without converting the whole error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { code, .. }
            | Self::Filesystem { code, .. }
            | Self::Database { code, .. }
            | Self::Secret { code, .. } => code,
        }
    }

    /// The diagnostic detail, when the error carries one. `Display` renders only
    /// the user-facing message, so call sites that fold an error into a per-item
    /// outcome string need this to keep the part that makes a failure
    /// actionable — the URL tried and what the upstream actually answered.
    pub fn details(&self) -> Option<&str> {
        match self {
            Self::Validation { details, .. }
            | Self::Filesystem { details, .. }
            | Self::Database { details, .. }
            | Self::Secret { details, .. } => details.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub recoverable: bool,
    pub operation_id: Option<String>,
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        match value {
            AppError::Validation {
                code,
                message,
                details,
                recoverable,
            }
            | AppError::Filesystem {
                code,
                message,
                details,
                recoverable,
            }
            | AppError::Database {
                code,
                message,
                details,
                recoverable,
            }
            | AppError::Secret {
                code,
                message,
                details,
                recoverable,
            } => {
                let operation_id = details.as_deref().and_then(extract_operation_id);
                Self {
                    code: code.to_string(),
                    message,
                    details,
                    recoverable,
                    operation_id,
                }
            }
        }
    }
}

fn extract_operation_id(details: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(details)
        .ok()?
        .get("operation_id")?
        .as_str()
        .map(str::to_string)
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Filesystem {
            code: "filesystem.io",
            message: "File operation failed".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::Validation {
            code: "validation.json",
            message: "JSON data is invalid".to_string(),
            details: Some(value.to_string()),
            recoverable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_extracts_group_operation_id_without_exposing_other_fields() {
        let error = ApiError::from(AppError::Filesystem {
            code: "filesystem.route_config_write",
            message: "Could not complete grouped configuration writes".to_string(),
            details: Some(
                serde_json::json!({
                    "operation_id": "operation-1",
                    "cause_code": "config.concurrent_modification"
                })
                .to_string(),
            ),
            recoverable: true,
        });

        assert_eq!(error.operation_id.as_deref(), Some("operation-1"));
        assert_eq!(error.code, "filesystem.route_config_write");
    }
}
