use crate::database::repositories::account_repository::AccountRepository;
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::import_repository::ImportRepository;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::error::AppError;
use crate::importers::example_json::parse_example_json;
use crate::importers::official_account_json::parse_official_account_json;
use crate::models::account::NewOfficialAccount;
use crate::models::batch::NewBatch;
use crate::models::import_job::ImportJob;
use crate::models::provider::NewProvider;
use crate::services::batch_service::BatchService;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

const MAX_DEEP_LINK_LENGTH: usize = 16 * 1024;
const MAX_DEEP_LINK_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleJsonImportRequest {
    pub batch_name: String,
    pub source_label: String,
    pub strategy: String,
    pub json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExampleJsonExportOutcome {
    pub json: String,
    pub provider_count: i64,
    pub account_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialAccountJsonImportRequest {
    pub batch_name: String,
    pub source_label: String,
    pub platform: String,
    pub json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLinkImportRequest {
    pub url: String,
}

pub struct ImportService;

impl ImportService {
    pub async fn import_example_json(
        pool: &SqlitePool,
        request: ExampleJsonImportRequest,
    ) -> Result<ImportJob, AppError> {
        if request.batch_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.import_batch_name_required",
                message: "Batch name is required for import".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let payload = parse_example_json(&request.json)?;
        let batch = BatchRepository::create(
            pool,
            NewBatch {
                name: request.batch_name.trim().to_string(),
                source: "example_json".to_string(),
                notes: Some(request.source_label.clone()),
            },
        )
        .await?;

        let job = ImportRepository::create_job(
            pool,
            "example_json",
            &request.source_label,
            Some(&batch.id),
            &request.strategy,
        )
        .await?;
        let mut success_count = 0_i64;

        for provider in payload.providers {
            let created =
                BatchService::create_provider(pool, provider, Some(batch.id.clone())).await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        for account in payload.accounts {
            let created =
                BatchService::create_official_account(pool, account, Some(batch.id.clone()))
                    .await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        let summary_json = serde_json::json!({
            "batch_id": batch.id,
            "created": success_count
        })
        .to_string();

        ImportRepository::complete_job(
            pool,
            &job.id,
            "completed",
            success_count,
            0,
            0,
            &summary_json,
        )
        .await
    }

    pub async fn export_example_json(
        pool: &SqlitePool,
    ) -> Result<ExampleJsonExportOutcome, AppError> {
        let providers = ProviderRepository::list(pool).await?;
        let accounts = AccountRepository::list(pool).await?;
        let provider_count = providers.len() as i64;
        let account_count = accounts.len() as i64;
        let export = serde_json::json!({
            "providers": providers.into_iter().map(|provider| NewProvider {
                name: provider.name,
                kind: provider.kind,
                base_url: provider.base_url,
                model_config_json: provider.model_config_json,
                target_options_json: provider.target_options_json,
                secret_ref: provider.secret_ref,
            }).collect::<Vec<_>>(),
            "accounts": accounts.into_iter().map(|account| NewOfficialAccount {
                platform: account.platform,
                display_name: account.display_name,
                email: account.email,
                plan: account.plan,
                account_metadata_json: account.account_metadata_json,
                secret_ref: account.secret_ref,
            }).collect::<Vec<_>>()
        });
        let json = serde_json::to_string_pretty(&export).map_err(|error| AppError::Validation {
            code: "validation.export_json",
            message: "Could not render export JSON".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

        Ok(ExampleJsonExportOutcome {
            json,
            provider_count,
            account_count,
        })
    }

    pub async fn import_official_account_json(
        pool: &SqlitePool,
        request: OfficialAccountJsonImportRequest,
    ) -> Result<ImportJob, AppError> {
        if request.batch_name.trim().is_empty() {
            return Err(AppError::Validation {
                code: "validation.import_batch_name_required",
                message: "Batch name is required for import".to_string(),
                details: None,
                recoverable: true,
            });
        }
        let platform = normalized_account_platform(&request.platform)?;
        let accounts = parse_official_account_json(&platform, &request.json)?;
        if accounts.is_empty() {
            return Err(AppError::Validation {
                code: "validation.account_import_empty",
                message: "Official account import must include at least one account".to_string(),
                details: None,
                recoverable: true,
            });
        }

        let batch = BatchRepository::create(
            pool,
            NewBatch {
                name: request.batch_name.trim().to_string(),
                source: "official_account_json".to_string(),
                notes: Some(request.source_label.clone()),
            },
        )
        .await?;
        let job = ImportRepository::create_job(
            pool,
            "official_account_json",
            &request.source_label,
            Some(&batch.id),
            "skip",
        )
        .await?;
        let mut success_count = 0_i64;

        for account in accounts {
            let created =
                BatchService::create_official_account(pool, account, Some(batch.id.clone()))
                    .await?;
            if !created.id.is_empty() {
                success_count += 1;
            }
        }

        let summary_json = serde_json::json!({
            "batch_id": batch.id,
            "platform": platform,
            "created": success_count
        })
        .to_string();

        ImportRepository::complete_job(
            pool,
            &job.id,
            "completed",
            success_count,
            0,
            0,
            &summary_json,
        )
        .await
    }

    pub async fn import_deep_link(
        pool: &SqlitePool,
        request: DeepLinkImportRequest,
    ) -> Result<ImportJob, AppError> {
        let parsed = parse_deep_link(&request.url)?;
        let json = decode_deep_link_payload(&parsed.payload)?;

        match parsed.action.as_str() {
            "example_json" => {
                Self::import_example_json(
                    pool,
                    ExampleJsonImportRequest {
                        batch_name: parsed
                            .params
                            .get("batch_name")
                            .cloned()
                            .unwrap_or_else(|| "Deep link import".to_string()),
                        source_label: parsed
                            .params
                            .get("source_label")
                            .cloned()
                            .unwrap_or_else(|| "deep link".to_string()),
                        strategy: parsed
                            .params
                            .get("strategy")
                            .cloned()
                            .unwrap_or_else(|| "skip".to_string()),
                        json,
                    },
                )
                .await
            }
            "official_account_json" => {
                let Some(platform) = parsed.params.get("platform").cloned() else {
                    return Err(AppError::Validation {
                        code: "validation.deep_link_platform_required",
                        message: "Official account deep links require a platform".to_string(),
                        details: None,
                        recoverable: true,
                    });
                };

                Self::import_official_account_json(
                    pool,
                    OfficialAccountJsonImportRequest {
                        batch_name: parsed
                            .params
                            .get("batch_name")
                            .cloned()
                            .unwrap_or_else(|| "Official account deep link".to_string()),
                        source_label: parsed
                            .params
                            .get("source_label")
                            .cloned()
                            .unwrap_or_else(|| "deep link".to_string()),
                        platform,
                        json,
                    },
                )
                .await
            }
            _ => Err(AppError::Validation {
                code: "validation.deep_link_route",
                message: "Deep link import route is not supported".to_string(),
                details: Some(parsed.action),
                recoverable: true,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDeepLink {
    action: String,
    payload: String,
    params: HashMap<String, String>,
}

fn parse_deep_link(url: &str) -> Result<ParsedDeepLink, AppError> {
    let url = url.trim();
    if url.len() > MAX_DEEP_LINK_LENGTH {
        return Err(AppError::Validation {
            code: "validation.deep_link_too_large",
            message: "Deep link is too large".to_string(),
            details: None,
            recoverable: true,
        });
    }

    let Some(rest) = url.strip_prefix("ai-switch://") else {
        return Err(AppError::Validation {
            code: "validation.deep_link_scheme",
            message: "Deep link must use the ai-switch:// scheme".to_string(),
            details: None,
            recoverable: true,
        });
    };
    let Some((route, query)) = rest.split_once('?') else {
        return Err(AppError::Validation {
            code: "validation.deep_link_query_required",
            message: "Deep link import query is required".to_string(),
            details: None,
            recoverable: true,
        });
    };
    let action = parse_deep_link_action(route)?;
    let params = parse_query_params(query)?;
    let Some(payload) = params.get("payload").cloned() else {
        return Err(AppError::Validation {
            code: "validation.deep_link_payload_required",
            message: "Deep link import payload is required".to_string(),
            details: None,
            recoverable: true,
        });
    };

    Ok(ParsedDeepLink {
        action,
        payload,
        params,
    })
}

fn parse_deep_link_action(route: &str) -> Result<String, AppError> {
    let route = route.trim_matches('/');
    let Some(action) = route.strip_prefix("import/") else {
        return Err(AppError::Validation {
            code: "validation.deep_link_route",
            message: "Deep link import route is not supported".to_string(),
            details: Some(route.to_string()),
            recoverable: true,
        });
    };
    let normalized = action.replace('-', "_");
    if matches!(
        normalized.as_str(),
        "example_json" | "official_account_json"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.deep_link_route",
        message: "Deep link import route is not supported".to_string(),
        details: Some(action.to_string()),
        recoverable: true,
    })
}

fn parse_query_params(query: &str) -> Result<HashMap<String, String>, AppError> {
    let mut params = HashMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok(params)
}

fn percent_decode(value: &str) -> Result<String, AppError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(percent_decode_error(value));
                }
                let high =
                    hex_value(bytes[index + 1]).ok_or_else(|| percent_decode_error(value))?;
                let low = hex_value(bytes[index + 2]).ok_or_else(|| percent_decode_error(value))?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8(decoded).map_err(|error| AppError::Validation {
        code: "validation.deep_link_percent_encoding",
        message: "Deep link query is not valid UTF-8".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn percent_decode_error(value: &str) -> AppError {
    AppError::Validation {
        code: "validation.deep_link_percent_encoding",
        message: "Deep link query percent encoding is invalid".to_string(),
        details: Some(value.to_string()),
        recoverable: true,
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_deep_link_payload(payload: &str) -> Result<String, AppError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .map_err(|error| AppError::Validation {
            code: "validation.deep_link_payload_base64",
            message: "Deep link import payload is not valid base64url".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        })?;

    if bytes.len() > MAX_DEEP_LINK_PAYLOAD_BYTES {
        return Err(AppError::Validation {
            code: "validation.deep_link_payload_too_large",
            message: "Deep link import payload is too large".to_string(),
            details: None,
            recoverable: true,
        });
    }

    String::from_utf8(bytes).map_err(|error| AppError::Validation {
        code: "validation.deep_link_payload_utf8",
        message: "Deep link import payload must be UTF-8 JSON".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })
}

fn normalized_account_platform(platform: &str) -> Result<String, AppError> {
    let normalized = platform.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "codex" | "claude" | "gemini" | "cursor" | "windsurf" | "zed" | "vscode"
    ) {
        return Ok(normalized);
    }

    Err(AppError::Validation {
        code: "validation.account_import_platform",
        message:
            "Official account import supports Codex, Claude, Gemini, Cursor, Windsurf, Zed, or VS Code"
                .to_string(),
        details: Some(platform.to_string()),
        recoverable: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};

    #[tokio::test]
    async fn import_example_json_creates_batch_items_and_job() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let request = ExampleJsonImportRequest {
            batch_name: "Batch 2026-07".to_string(),
            source_label: "inline fixture".to_string(),
            strategy: "skip".to_string(),
            json: r#"{
              "providers": [{"name":"Acme Claude","kind":"openai_compatible","base_url":"https://api.example.com/v1","model_config_json":"{}","target_options_json":"{}","secret_ref":"secret://provider/acme"}],
              "accounts": [{"platform":"codex","display_name":"Team Account","email":"team@example.com","plan":"team","account_metadata_json":"{}","secret_ref":"secret://account/team"}]
            }"#
            .to_string(),
        };

        let job = ImportService::import_example_json(&pool, request)
            .await
            .expect("import");

        assert_eq!(job.status, "completed");
        assert_eq!(job.success_count, 2);
        assert_eq!(job.failure_count, 0);
    }

    #[tokio::test]
    async fn export_example_json_returns_reimportable_shape() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        ImportService::import_example_json(
            &pool,
            ExampleJsonImportRequest {
                batch_name: "Export Batch".to_string(),
                source_label: "inline fixture".to_string(),
                strategy: "skip".to_string(),
                json: r#"{
                  "providers": [{"name":"Acme Provider","kind":"openai_compatible","base_url":"https://api.example.com/v1","model_config_json":"{}","target_options_json":"{}","secret_ref":"env://ACME_API_KEY"}],
                  "accounts": [{"platform":"codex","display_name":"Team Account","email":"team@example.com","plan":"team","account_metadata_json":"{}","secret_ref":"secret://account/team"}]
                }"#
                .to_string(),
            },
        )
        .await
        .expect("import");

        let exported = ImportService::export_example_json(&pool)
            .await
            .expect("export");
        let value: serde_json::Value = serde_json::from_str(&exported.json).expect("json");

        assert_eq!(exported.provider_count, 1);
        assert_eq!(exported.account_count, 1);
        assert_eq!(value["providers"][0]["name"], "Acme Provider");
        assert_eq!(value["accounts"][0]["display_name"], "Team Account");
        parse_example_json(&exported.json).expect("reimportable");
    }

    #[tokio::test]
    async fn import_official_account_json_creates_platform_accounts_and_job() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let job = ImportService::import_official_account_json(
            &pool,
            OfficialAccountJsonImportRequest {
                batch_name: "Codex Accounts".to_string(),
                source_label: "account paste".to_string(),
                platform: "codex".to_string(),
                json: r#"{"accounts":[{"display_name":"Team Codex","email":"team@example.com","plan":"team","metadata":{"workspace":"eng"},"secret_ref":"secret://account/team"}]}"#.to_string(),
            },
        )
        .await
        .expect("import");
        let accounts = AccountRepository::list(&pool).await.expect("accounts");

        assert_eq!(job.status, "completed");
        assert_eq!(job.source_type, "official_account_json");
        assert_eq!(job.success_count, 1);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].platform, "codex");
        assert_eq!(accounts[0].display_name, "Team Codex");
    }

    #[tokio::test]
    async fn import_official_account_json_accepts_ide_platforms() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let job = ImportService::import_official_account_json(
            &pool,
            OfficialAccountJsonImportRequest {
                batch_name: "IDE Accounts".to_string(),
                source_label: "cursor paste".to_string(),
                platform: "Cursor".to_string(),
                json: r#"{"accounts":[{"display_name":"Team Cursor","email":"cursor@example.com","metadata":{"workspace":"ide"}}]}"#.to_string(),
            },
        )
        .await
        .expect("import");
        let accounts = AccountRepository::list(&pool).await.expect("accounts");

        assert_eq!(job.success_count, 1);
        assert_eq!(accounts[0].platform, "cursor");
        assert_eq!(accounts[0].display_name, "Team Cursor");
    }

    #[tokio::test]
    async fn import_official_account_json_rejects_raw_secret_metadata() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let error = ImportService::import_official_account_json(
            &pool,
            OfficialAccountJsonImportRequest {
                batch_name: "Unsafe Accounts".to_string(),
                source_label: "account paste".to_string(),
                platform: "codex".to_string(),
                json:
                    r#"{"accounts":[{"display_name":"Unsafe","metadata":{"refresh_token":"raw"}}]}"#
                        .to_string(),
            },
        )
        .await
        .expect_err("error");

        assert_eq!(error.code(), "validation.account_import_raw_secret");
    }

    #[tokio::test]
    async fn import_deep_link_imports_example_json_payload() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let payload = r#"{
          "providers": [{"name":"Deep Link Provider","kind":"openai_compatible","base_url":"https://api.example.com/v1","model_config_json":"{}","target_options_json":"{}","secret_ref":"env://DEEP_LINK_API_KEY"}],
          "accounts": []
        }"#;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let url = format!(
            "ai-switch://import/example_json?batch_name=Deep%20Link&source_label=shared&strategy=skip&payload={encoded}"
        );

        let job = ImportService::import_deep_link(&pool, DeepLinkImportRequest { url })
            .await
            .expect("deep link import");
        let providers = ProviderRepository::list(&pool).await.expect("providers");

        assert_eq!(job.source_type, "example_json");
        assert_eq!(job.success_count, 1);
        assert_eq!(providers[0].name, "Deep Link Provider");
    }

    #[tokio::test]
    async fn import_deep_link_imports_official_account_payload() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let payload = r#"{"accounts":[{"display_name":"Deep Link Gemini","email":"team@example.com","metadata":{"workspace":"eng"},"secret_ref":"secret://account/gemini"}]}"#;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let url = format!(
            "ai-switch://import/official-account-json?batch_name=Accounts&source_label=shared&platform=gemini&payload={encoded}"
        );

        let job = ImportService::import_deep_link(&pool, DeepLinkImportRequest { url })
            .await
            .expect("deep link import");
        let accounts = AccountRepository::list(&pool).await.expect("accounts");

        assert_eq!(job.source_type, "official_account_json");
        assert_eq!(job.success_count, 1);
        assert_eq!(accounts[0].platform, "gemini");
        assert_eq!(accounts[0].display_name, "Deep Link Gemini");
    }

    #[tokio::test]
    async fn import_deep_link_rejects_malformed_links() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let wrong_scheme = ImportService::import_deep_link(
            &pool,
            DeepLinkImportRequest {
                url: "https://example.com/import/example_json?payload=e30".to_string(),
            },
        )
        .await
        .expect_err("wrong scheme");
        assert_eq!(wrong_scheme.code(), "validation.deep_link_scheme");

        let bad_payload = ImportService::import_deep_link(
            &pool,
            DeepLinkImportRequest {
                url: "ai-switch://import/example_json?payload=not base64".to_string(),
            },
        )
        .await
        .expect_err("bad payload");
        assert_eq!(bad_payload.code(), "validation.deep_link_payload_base64");
    }
}
