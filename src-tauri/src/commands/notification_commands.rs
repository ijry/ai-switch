use crate::error::ApiError;
use crate::models::notification::{NotificationChannelKind, NotificationConfig};
use crate::services::notification_service;

/// Test a notification channel by sending a test message.
#[tauri::command]
pub async fn test_notification(kind: NotificationChannelKind) -> Result<(), ApiError> {
    notification_service::test_channel(&kind)
        .await
        .map_err(|e| ApiError::from(crate::error::AppError::Validation {
            code: "notification.test_failed",
            message: format!("Notification test failed: {e}"),
            details: None,
            recoverable: true,
        }))
}
