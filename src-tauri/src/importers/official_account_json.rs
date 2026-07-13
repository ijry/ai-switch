use crate::error::AppError;
use crate::models::account::NewOfficialAccount;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OfficialAccountPayload {
    #[serde(default)]
    accounts: Vec<OfficialAccountItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OfficialAccountItem {
    display_name: String,
    email: Option<String>,
    plan: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    account_metadata_json: Option<String>,
    secret_ref: Option<String>,
}

pub fn parse_official_account_json(
    platform: &str,
    input: &str,
) -> Result<Vec<NewOfficialAccount>, AppError> {
    let payload: OfficialAccountPayload =
        serde_json::from_str(input).map_err(|error| AppError::Validation {
            code: "validation.account_import_json",
            message: "Official account import JSON is invalid".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    payload
        .accounts
        .into_iter()
        .map(|item| {
            let metadata_json = account_metadata_json(&item)?;
            Ok(NewOfficialAccount {
                platform: platform.to_string(),
                display_name: item.display_name,
                email: item.email,
                plan: item.plan,
                account_metadata_json: metadata_json,
                secret_ref: item.secret_ref,
            })
        })
        .collect()
}

fn account_metadata_json(item: &OfficialAccountItem) -> Result<String, AppError> {
    if let Some(metadata_json) = &item.account_metadata_json {
        let value: Value =
            serde_json::from_str(metadata_json).map_err(|error| AppError::Validation {
                code: "validation.account_import_metadata_json",
                message: "Account metadata JSON is invalid".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
        reject_sensitive_metadata(&value)?;
        return Ok(metadata_json.trim().to_string());
    }

    let metadata = item
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    reject_sensitive_metadata(&metadata)?;
    serde_json::to_string(&metadata).map_err(|error| AppError::Validation {
        code: "validation.account_import_metadata_json",
        message: "Account metadata JSON is invalid".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn reject_sensitive_metadata(value: &Value) -> Result<(), AppError> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let normalized = key.to_lowercase().replace('-', "_");
                if normalized.contains("token")
                    || normalized.contains("api_key")
                    || normalized.contains("apikey")
                    || normalized.contains("password")
                    || normalized.contains("secret")
                {
                    return Err(AppError::Validation {
                        code: "validation.account_import_raw_secret",
                        message: "Account import metadata must not contain raw credential fields"
                            .to_string(),
                        details: Some(key.to_string()),
                        recoverable: true,
                    });
                }
                reject_sensitive_metadata(value)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                reject_sensitive_metadata(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_bundle_for_platform() {
        let accounts = parse_official_account_json(
            "codex",
            r#"{"accounts":[{"display_name":"Team Codex","email":"team@example.com","plan":"team","metadata":{"workspace":"eng"},"secret_ref":"secret://account/team"}]}"#,
        )
        .expect("accounts");

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].platform, "codex");
        assert_eq!(accounts[0].display_name, "Team Codex");
        assert_eq!(accounts[0].account_metadata_json, "{\"workspace\":\"eng\"}");
        assert_eq!(
            accounts[0].secret_ref.as_deref(),
            Some("secret://account/team")
        );
    }

    #[test]
    fn rejects_sensitive_metadata_keys() {
        let error = parse_official_account_json(
            "codex",
            r#"{"accounts":[{"display_name":"Unsafe","metadata":{"access_token":"raw"}}]}"#,
        )
        .expect_err("error");

        assert_eq!(error.code(), "validation.account_import_raw_secret");
    }
}
