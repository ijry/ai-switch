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
            } => Self {
                code: code.to_string(),
                message,
                details,
                recoverable,
                operation_id: None,
            },
        }
    }
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
