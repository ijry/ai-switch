use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_credential_transfer_repository::get_or_create_installation_id;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::platform::PlatformId;
use crate::models::route_credential::{ModelMapping, RouteCredential};
use crate::models::route_credential_transfer::{
    ExportRouteCredentialsInput, RouteCredentialExportCounts, RouteCredentialExportResult,
    RouteCredentialSchemeLink, RouteCredentialTransferIssue, TRANSFER_MAX_BYTES,
    TRANSFER_MAX_EXPORT_IDS,
};
use crate::services::cpa_export_service::project_credential;
use crate::services::deeplink_service::{build_aiswitch_import_url, DeepLinkBuildInput};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

pub async fn export_route_credentials(
    pool: &SqlitePool,
    input: ExportRouteCredentialsInput,
) -> Result<RouteCredentialExportResult, AppError> {
    let suggested_file_name =
        suggested_export_file_name(&input.selection_context.platform, Utc::now());
    let mut result = RouteCredentialExportResult {
        json_text: None,
        suggested_file_name,
        counts: RouteCredentialExportCounts::default(),
        scheme_links: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let platform = match PlatformId::parse(&input.selection_context.platform) {
        Ok(platform) => platform,
        Err(_) => {
            result
                .errors
                .push(selection_issue("transfer.platform_unknown"));
            return Ok(result);
        }
    };
    if input.credential_ids.is_empty() {
        result
            .errors
            .push(selection_issue("transfer.selection_empty"));
        return Ok(result);
    }
    if input.credential_ids.len() > TRANSFER_MAX_EXPORT_IDS {
        result
            .errors
            .push(selection_issue("transfer.selection_too_large"));
        return Ok(result);
    }

    let mut seen = HashSet::with_capacity(input.credential_ids.len());
    let unique_entries = input
        .credential_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| seen.insert((*id).as_str()))
        .map(|(index, id)| (index, id.clone()))
        .collect::<Vec<_>>();
    let unique_ids = unique_entries
        .iter()
        .map(|(_, id)| id.clone())
        .collect::<Vec<_>>();
    let mut selection_context = input.selection_context.clone();
    selection_context.platform = platform.as_str().to_string();
    let credentials =
        RouteCredentialRepository::list_by_ids(pool, &unique_ids, &selection_context).await?;
    let loaded_ids = credentials
        .iter()
        .map(|credential| credential.id.as_str())
        .collect::<HashSet<_>>();
    for (index, id) in &unique_entries {
        if !loaded_ids.contains(id.as_str()) {
            result.errors.push(RouteCredentialTransferIssue {
                item_index: Some(*index),
                display_name: None,
                code: "transfer.credential_not_found_or_out_of_context".to_string(),
                field: None,
            });
        }
    }
    if !result.errors.is_empty() {
        return Ok(result);
    }

    let memberships =
        RoutePoolRepository::pool_membership_map(pool, platform.as_str(), &unique_ids).await?;
    let instance_id = get_or_create_installation_id(pool).await?;
    let mut payloads = Vec::with_capacity(credentials.len());
    let mut projected = Vec::with_capacity(credentials.len());
    for credential in &credentials {
        match credential.kind.trim().to_ascii_lowercase().as_str() {
            "official" => result.counts.official += 1,
            "api" => result.counts.api += 1,
            _ => {}
        }
        match project_credential(
            credential,
            &instance_id,
            memberships.contains(&credential.id),
            input.include_enhanced_metadata,
        ) {
            Ok(item) => {
                result.warnings.extend(item.warnings.clone());
                payloads.push(item.payload.clone());
                projected.push((credential, item));
            }
            Err(issue) => result.errors.push(issue),
        }
    }
    result.counts.total = credentials.len();
    if !result.errors.is_empty() {
        return Ok(result);
    }

    for (credential, _) in projected {
        if credential.kind.trim().eq_ignore_ascii_case("api") {
            let link = build_scheme_link(credential);
            if let Some(code) = link.issue_code.as_deref() {
                result.warnings.push(RouteCredentialTransferIssue {
                    item_index: None,
                    display_name: Some(credential.display_name.clone()),
                    code: code.to_string(),
                    field: None,
                });
            }
            result.scheme_links.push(link);
        }
    }

    let mut json_text = serde_json::to_string_pretty(&payloads)?;
    json_text.push('\n');
    if json_text.len() > TRANSFER_MAX_BYTES {
        result
            .errors
            .push(selection_issue("transfer.export_too_large"));
        result.scheme_links.clear();
        return Ok(result);
    }
    result.json_text = Some(json_text);
    Ok(result)
}

pub fn suggested_export_file_name(platform: &str, now: DateTime<Utc>) -> String {
    let platform = PlatformId::parse(platform)
        .map(PlatformId::as_str)
        .unwrap_or("unknown");
    format!(
        "ai-switch-{platform}-route-credentials-{}.json",
        now.format("%Y%m%d-%H%M%S")
    )
}

fn selection_issue(code: &str) -> RouteCredentialTransferIssue {
    RouteCredentialTransferIssue {
        item_index: None,
        display_name: None,
        code: code.to_string(),
        field: None,
    }
}

fn build_scheme_link(credential: &RouteCredential) -> RouteCredentialSchemeLink {
    let outcome = (|| -> Result<String, String> {
        let secret =
            serde_json::from_str::<HashMap<String, Value>>(&credential.secret_payload_json)
                .map_err(|_| "deeplink_export.payload_unsupported".to_string())?;
        let config = serde_json::from_str::<HashMap<String, Value>>(&credential.config_json)
            .map_err(|_| "deeplink_export.payload_unsupported".to_string())?;
        let api_key = secret
            .get("api_key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let base_url = config
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let interface_format = config
            .get("interface_format")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let model_mappings = config
            .get("model_mappings")
            .cloned()
            .map(serde_json::from_value::<Vec<ModelMapping>>)
            .transpose()
            .map_err(|_| "deeplink_export.model_mappings_unsupported".to_string())?
            .unwrap_or_default();
        let headers = config.get("headers").cloned().unwrap_or(Value::Null);
        let api_key_field = config.get("api_key_field").and_then(Value::as_str);
        let responses_custom_tool_compat = config
            .get("responses_custom_tool_compat")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        build_aiswitch_import_url(&DeepLinkBuildInput {
            platform: &credential.platform,
            display_name: &credential.display_name,
            base_url,
            api_key,
            interface_format,
            model_mappings: &model_mappings,
            headers: &headers,
            api_key_field,
            responses_custom_tool_compat,
        })
    })();
    match outcome {
        Ok(url) => RouteCredentialSchemeLink {
            credential_id: credential.id.clone(),
            display_name: credential.display_name.clone(),
            url: Some(url),
            issue_code: None,
        },
        Err(code) => {
            let code = if code.starts_with("deeplink_export.") {
                code
            } else {
                "deeplink_export.payload_unsupported".to_string()
            };
            RouteCredentialSchemeLink {
                credential_id: credential.id.clone(),
                display_name: credential.display_name.clone(),
                url: None,
                issue_code: Some(code),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::models::route_credential::{RouteCredential, RouteCredentialPoolScope};
    use crate::models::route_credential_transfer::RouteCredentialSelectionContext;
    use chrono::TimeZone;
    use serde_json::json;

    async fn pool() -> SqlitePool {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        pool
    }

    async fn insert_credential(
        pool: &SqlitePool,
        platform: &str,
        kind: &str,
        name: &str,
        secret: Value,
        config: Value,
    ) -> RouteCredential {
        RouteCredentialRepository::create(
            pool,
            platform,
            kind,
            name,
            None,
            "ok",
            None,
            &secret.to_string(),
            &config.to_string(),
            "{}",
        )
        .await
        .expect("credential")
    }

    async fn api(pool: &SqlitePool, platform: &str, name: &str) -> RouteCredential {
        let interface_format = match platform {
            "claude" => "anthropic",
            "gemini" => "gemini",
            "grok" => "openai",
            _ => "openai-responses",
        };
        insert_credential(
            pool,
            platform,
            "api",
            name,
            json!({"api_key": "sk-test"}),
            json!({
                "base_url": "https://api.openai.com/v1",
                "interface_format": interface_format,
                "model_mappings": []
            }),
        )
        .await
    }

    fn input(
        platform: &str,
        scope: RouteCredentialPoolScope,
        ids: Vec<String>,
        enhanced: bool,
    ) -> ExportRouteCredentialsInput {
        ExportRouteCredentialsInput {
            selection_context: RouteCredentialSelectionContext {
                platform: platform.to_string(),
                pool_scope: scope,
            },
            credential_ids: ids,
            include_enhanced_metadata: enhanced,
        }
    }

    fn error_codes(result: &RouteCredentialExportResult) -> Vec<&str> {
        result
            .errors
            .iter()
            .map(|issue| issue.code.as_str())
            .collect()
    }

    #[test]
    fn suggested_file_name_is_platform_scoped_and_utc_stable() {
        let now = Utc.with_ymd_and_hms(2026, 8, 5, 1, 2, 3).unwrap();
        assert_eq!(
            suggested_export_file_name("Claude", now),
            "ai-switch-claude-route-credentials-20260805-010203.json"
        );
    }

    #[test]
    fn selection_issues_are_redacted() {
        let issue = selection_issue("transfer.selection_empty");
        assert_eq!(issue.item_index, None);
        assert_eq!(issue.display_name, None);
        assert_eq!(issue.field, None);
    }

    #[tokio::test]
    async fn rejects_empty_oversized_and_unknown_platform_selections_without_json() {
        let pool = pool().await;
        let empty = export_route_credentials(
            &pool,
            input("codex", RouteCredentialPoolScope::OutOfPool, vec![], true),
        )
        .await
        .unwrap();
        assert_eq!(error_codes(&empty), ["transfer.selection_empty"]);
        assert!(empty.json_text.is_none());

        let oversized = export_route_credentials(
            &pool,
            input(
                "codex",
                RouteCredentialPoolScope::OutOfPool,
                (0..=TRANSFER_MAX_EXPORT_IDS)
                    .map(|index| format!("id-{index}"))
                    .collect(),
                true,
            ),
        )
        .await
        .unwrap();
        assert_eq!(error_codes(&oversized), ["transfer.selection_too_large"]);
        assert!(oversized.json_text.is_none());

        let unknown = export_route_credentials(
            &pool,
            input(
                "secret-platform",
                RouteCredentialPoolScope::OutOfPool,
                vec!["id".into()],
                true,
            ),
        )
        .await
        .unwrap();
        assert_eq!(error_codes(&unknown), ["transfer.platform_unknown"]);
        assert!(unknown.json_text.is_none());
    }

    #[tokio::test]
    async fn missing_wrong_platform_and_wrong_pool_scope_are_all_or_nothing() {
        let pool = pool().await;
        let valid = api(&pool, "codex", "Valid").await;
        let wrong_platform = api(&pool, "claude", "Wrong platform").await;
        let pool_member = api(&pool, "codex", "Pool member").await;
        RoutePoolRepository::replace_members(&pool, "codex", &[pool_member.id.clone()])
            .await
            .unwrap();

        for ids in [
            vec![valid.id.clone(), "missing".into()],
            vec![valid.id.clone(), wrong_platform.id.clone()],
            vec![valid.id.clone(), pool_member.id.clone()],
        ] {
            let result = export_route_credentials(
                &pool,
                input("codex", RouteCredentialPoolScope::OutOfPool, ids, true),
            )
            .await
            .unwrap();
            assert!(result.json_text.is_none());
            assert!(result.scheme_links.is_empty());
            assert_eq!(
                error_codes(&result),
                ["transfer.credential_not_found_or_out_of_context"]
            );
            assert_eq!(result.errors[0].item_index, Some(1));
            assert!(result.errors[0].display_name.is_none());
        }
    }

    #[tokio::test]
    async fn successful_export_deduplicates_orders_counts_formats_and_preserves_source_ids() {
        let pool = pool().await;
        let official = insert_credential(
            &pool,
            "codex",
            "official",
            "Official",
            json!({"access_token": "access"}),
            json!({"raw_type": "codex", "raw": {"type": "codex"}}),
        )
        .await;
        let api = api(&pool, "codex", "API").await;
        sqlx::query("UPDATE route_credentials SET sort_order = 20 WHERE id = ?")
            .bind(&official.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE route_credentials SET sort_order = 10 WHERE id = ?")
            .bind(&api.id)
            .execute(&pool)
            .await
            .unwrap();

        for enhanced in [true, false] {
            let result = export_route_credentials(
                &pool,
                input(
                    "openai",
                    RouteCredentialPoolScope::OutOfPool,
                    vec![official.id.clone(), api.id.clone(), official.id.clone()],
                    enhanced,
                ),
            )
            .await
            .unwrap();
            assert!(result.errors.is_empty());
            assert_eq!(
                result.counts,
                RouteCredentialExportCounts {
                    total: 2,
                    official: 1,
                    api: 1
                }
            );
            assert_eq!(result.scheme_links.len(), 1);
            assert!(result.scheme_links[0]
                .url
                .as_deref()
                .is_some_and(|url| url.starts_with("aiswitch://v1/import?")));
            let text = result.json_text.as_deref().unwrap();
            assert!(text.starts_with("[\n  {"));
            assert!(text.ends_with('\n'));
            assert!(!text.ends_with("\n\n"));
            let values: Vec<Value> = serde_json::from_str(text).unwrap();
            assert_eq!(values.len(), 2);
            assert_eq!(values[0]["x-ai-switch"]["source_credential_id"], api.id);
            assert_eq!(
                values[1]["x-ai-switch"]["source_credential_id"],
                official.id
            );
            for value in &values {
                let metadata = &value["x-ai-switch"];
                assert_eq!(metadata["format"], "ai-switch.route-credential");
                assert_eq!(metadata["schema_version"], 1);
                assert!(metadata["source_instance_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty()));
                assert!(metadata["source_credential_id"].as_str().is_some());
                if metadata["kind"] == "api" {
                    assert!(!value.as_object().unwrap().contains_key("type"));
                }
                assert_eq!(metadata.get("display_name").is_some(), enhanced);
            }
            assert_eq!(values[0]["x-ai-switch"]["cpa_section"], "codex-api-key");
        }
    }

    #[tokio::test]
    async fn blocking_projection_errors_remove_json_while_link_failures_only_warn() {
        let pool = pool().await;
        let valid = api(&pool, "codex", "Valid").await;
        let invalid = insert_credential(
            &pool,
            "codex",
            "api",
            "Invalid",
            json!({}),
            json!({"base_url": "https://api.openai.com/v1", "interface_format": "openai-responses"}),
        )
        .await;
        let blocked = export_route_credentials(
            &pool,
            input(
                "codex",
                RouteCredentialPoolScope::OutOfPool,
                vec![valid.id, invalid.id],
                true,
            ),
        )
        .await
        .unwrap();
        assert!(blocked.json_text.is_none());
        assert!(blocked.scheme_links.is_empty());
        assert_eq!(error_codes(&blocked), ["transfer.api_key_required"]);

        let lossy = insert_credential(
            &pool,
            "codex",
            "api",
            "Lossy",
            json!({"api_key": "sk-lossy"}),
            json!({
                "base_url": "https://api.openai.com/v1",
                "interface_format": "openai-responses",
                "headers": {"X-Secret-Mode": "1"},
                "future_field": "ignored"
            }),
        )
        .await;
        let warned = export_route_credentials(
            &pool,
            input(
                "codex",
                RouteCredentialPoolScope::OutOfPool,
                vec![lossy.id],
                true,
            ),
        )
        .await
        .unwrap();
        assert!(warned.json_text.is_some());
        assert_eq!(warned.scheme_links.len(), 1);
        assert!(warned.scheme_links[0].url.is_none());
        assert_eq!(
            warned.scheme_links[0].issue_code.as_deref(),
            Some("deeplink_export.headers_unsupported")
        );
        assert!(warned
            .warnings
            .iter()
            .any(|issue| issue.code == "transfer.api_config_field_ignored"));
        assert!(warned
            .warnings
            .iter()
            .any(|issue| issue.code == "deeplink_export.headers_unsupported"));
    }

    #[tokio::test]
    async fn output_over_eight_mib_is_blocking_and_returns_no_links() {
        let pool = pool().await;
        let huge = insert_credential(
            &pool,
            "codex",
            "api",
            &"x".repeat(TRANSFER_MAX_BYTES + 1),
            json!({"api_key": "sk-huge"}),
            json!({"base_url": "https://openrouter.ai/api/v1", "interface_format": "openai-responses"}),
        )
        .await;
        let result = export_route_credentials(
            &pool,
            input(
                "codex",
                RouteCredentialPoolScope::OutOfPool,
                vec![huge.id],
                true,
            ),
        )
        .await
        .unwrap();
        assert!(result.json_text.is_none());
        assert!(result.scheme_links.is_empty());
        assert_eq!(error_codes(&result), ["transfer.export_too_large"]);
    }
}
