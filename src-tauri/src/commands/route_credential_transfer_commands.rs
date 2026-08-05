use crate::app_state::AppState;
use crate::config_writer::ConfigWriter;
use crate::error::{ApiError, AppError};
use crate::models::route_credential_transfer::{
    ExportRouteCredentialsInput, ImportRouteCredentialsInput, PreviewRouteCredentialImportInput,
    RouteCredentialExportResult, RouteCredentialImportOutcome, RouteCredentialImportPreview,
    TRANSFER_MAX_BYTES, TRANSFER_MAX_ITEMS,
};
use crate::services::route_credential_transfer_import_service;
use crate::services::route_credential_transfer_service;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn export_route_credentials(
    state: State<'_, AppState>,
    input: ExportRouteCredentialsInput,
) -> Result<RouteCredentialExportResult, ApiError> {
    route_credential_transfer_service::export_route_credentials(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn preview_route_credential_import(
    state: State<'_, AppState>,
    input: PreviewRouteCredentialImportInput,
) -> Result<RouteCredentialImportPreview, ApiError> {
    route_credential_transfer_import_service::preview_route_credential_import(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn import_route_credentials(
    state: State<'_, AppState>,
    input: ImportRouteCredentialsInput,
) -> Result<RouteCredentialImportOutcome, ApiError> {
    route_credential_transfer_import_service::import_route_credentials(&state.pool, input)
        .await
        .map_err(ApiError::from)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveRouteCredentialExportResult {
    pub cancelled: bool,
    pub file_name: Option<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_route_credential_export(
    app: tauri::AppHandle,
    suggested_file_name: String,
    json_text: String,
) -> Result<SaveRouteCredentialExportResult, ApiError> {
    let suggested_file_name = normalize_suggested_file_name(&suggested_file_name);
    let selected = app
        .dialog()
        .file()
        .set_file_name(suggested_file_name)
        .add_filter("JSON", &["json"])
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(cancelled_save_result());
    };
    let selected = selected.into_path().map_err(|error| {
        ApiError::from(AppError::Validation {
            code: "transfer.export_path_invalid",
            message: "The selected export path is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })
    })?;
    let path = write_export_file(selected, &json_text)
        .await
        .map_err(ApiError::from)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned);

    Ok(SaveRouteCredentialExportResult {
        cancelled: false,
        file_name,
    })
}

fn normalize_suggested_file_name(value: &str) -> String {
    let basename = value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .unwrap_or("ai-switch-route-credentials");
    normalize_json_file_name(basename)
}

fn normalize_json_file_name(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if !lower.ends_with(".json") {
        return format!("{value}.json");
    }

    let mut suffix_count = 0;
    let mut remaining = lower.as_str();
    while let Some(stripped) = remaining.strip_suffix(".json") {
        suffix_count += 1;
        remaining = stripped;
    }
    if suffix_count == 1 {
        value.to_string()
    } else {
        format!("{}.json", &value[..remaining.len()])
    }
}

fn normalize_json_path(mut path: PathBuf) -> PathBuf {
    if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
        path.set_file_name(normalize_json_file_name(file_name));
    } else {
        path.as_mut_os_string().push(".json");
    }
    path
}

fn validate_export_json(bytes: &[u8]) -> Result<&str, AppError> {
    if bytes.len() > TRANSFER_MAX_BYTES {
        return Err(transfer_validation_error(
            "transfer.export_too_large",
            "The route credential export exceeds the maximum size",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        transfer_validation_error(
            "transfer.export_invalid_utf8",
            "The route credential export is not valid UTF-8",
        )
    })?;
    let value: Value = serde_json::from_str(text).map_err(|error| AppError::Validation {
        code: "transfer.export_invalid_json",
        message: "The route credential export is not valid JSON".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    let items = value.as_array().ok_or_else(|| {
        transfer_validation_error(
            "transfer.export_root_not_array",
            "The route credential export must be a top-level array",
        )
    })?;
    if items.len() > TRANSFER_MAX_ITEMS {
        return Err(transfer_validation_error(
            "transfer.export_too_many_items",
            "The route credential export contains too many items",
        ));
    }
    Ok(text)
}

async fn write_export_file(path: PathBuf, json_text: &str) -> Result<PathBuf, AppError> {
    let json_text = validate_export_json(json_text.as_bytes())?;
    let path = normalize_json_path(path);
    ConfigWriter::write_atomic(&path, json_text).await?;
    Ok(path)
}

fn cancelled_save_result() -> SaveRouteCredentialExportResult {
    SaveRouteCredentialExportResult {
        cancelled: true,
        file_name: None,
    }
}

fn transfer_validation_error(code: &'static str, message: &str) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details: None,
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::models::route_credential_transfer::{TRANSFER_MAX_BYTES, TRANSFER_MAX_ITEMS};
    use serde_json::Value;
    use tempfile::tempdir;

    fn assert_validation_code(error: AppError, expected: &'static str) {
        assert!(matches!(
            error,
            AppError::Validation { code, .. } if code == expected
        ));
    }

    #[test]
    fn suggested_name_is_a_basename_with_one_json_suffix() {
        assert_eq!(
            normalize_suggested_file_name("C:/unsafe/credentials"),
            "credentials.json"
        );
        assert_eq!(
            normalize_suggested_file_name("credentials.JSON"),
            "credentials.JSON"
        );
        assert_eq!(
            normalize_suggested_file_name("credentials.txt"),
            "credentials.txt.json"
        );
        assert_eq!(
            normalize_suggested_file_name("  "),
            "ai-switch-route-credentials.json"
        );
    }

    #[test]
    fn selected_path_is_normalized_without_accepting_a_frontend_path() {
        let selected = std::path::PathBuf::from("C:/chosen/export");
        assert_eq!(
            normalize_json_path(selected),
            std::path::PathBuf::from("C:/chosen/export.json")
        );
    }

    #[test]
    fn export_json_validation_enforces_utf8_array_and_limits() {
        assert_eq!(validate_export_json(b"[]").unwrap(), "[]");
        assert_validation_code(
            validate_export_json(&[0xff]).unwrap_err(),
            "transfer.export_invalid_utf8",
        );
        assert_validation_code(
            validate_export_json(br#"{"secret":"value"}"#).unwrap_err(),
            "transfer.export_root_not_array",
        );

        let too_many = serde_json::to_vec(&vec![Value::Null; TRANSFER_MAX_ITEMS + 1]).unwrap();
        assert_validation_code(
            validate_export_json(&too_many).unwrap_err(),
            "transfer.export_too_many_items",
        );
        assert_validation_code(
            validate_export_json(&vec![b' '; TRANSFER_MAX_BYTES + 1]).unwrap_err(),
            "transfer.export_too_large",
        );
    }

    #[test]
    fn dialog_cancellation_is_a_success_result() {
        assert_eq!(
            cancelled_save_result(),
            SaveRouteCredentialExportResult {
                cancelled: true,
                file_name: None,
            }
        );
    }

    #[tokio::test]
    async fn validated_export_is_written_atomically_to_the_dialog_path() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("credentials");
        let normalized = write_export_file(target, "[{\"token\":\"secret\"}]")
            .await
            .unwrap();

        assert_eq!(normalized, dir.path().join("credentials.json"));
        assert_eq!(
            tokio::fs::read_to_string(&normalized).await.unwrap(),
            "[{\"token\":\"secret\"}]"
        );
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            assert!(!entry.file_name().to_string_lossy().ends_with(".tmp"));
        }
    }
}
