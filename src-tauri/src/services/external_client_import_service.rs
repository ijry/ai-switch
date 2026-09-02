//! Preview and commit for "import from another client".
//!
//! Only cc-switch is supported today; the client id is a parameter rather than
//! baked in so a second reader can be added without reshaping the command
//! surface.
//!
//! Dedupe rule: `(external_source_client, external_source_id)` is unique, so an
//! account already imported from a given source record is *overwritten* rather
//! than duplicated. That is the deliberate difference from the AI Switch
//! transfer import, which refuses edited duplicates — here the external client
//! is the authority for the fields it owns.

use crate::database::repositories::route_credential_repository::{
    ExternalSourceMatch, ExternalSourceRef, RouteCredentialRepository,
};
use crate::error::AppError;
use crate::models::external_client_import::{
    ExternalClientAccountPreviewItem, ExternalClientImportOutcome, ExternalClientImportPreview,
    ExternalClientImportPreviewCounts, ImportExternalClientAccountsInput,
    PreviewExternalClientImportInput, EXTERNAL_CLIENT_CC_SWITCH,
};
use crate::models::platform::{PlatformId, PlatformOperation};
use crate::services::cc_switch_import_service::{
    self as cc_switch, ExternalClientProvider, ExtractIssue, ExtractedApiCredential,
};
// Reuse the deep-link masker rather than growing a second one: the preview must
// mask exactly like every other credential surface.
use crate::services::deeplink_service::mask_api_key;
use crate::services::platform_capability_service::PlatformCapabilityService;
use crate::services::route_preview_service::RoutePreviewService;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// Clients this build can read. Kept as a list so the frontend can be told what
/// is available rather than hard-coding the same knowledge twice.
pub fn supported_clients() -> Vec<&'static str> {
    vec![EXTERNAL_CLIENT_CC_SWITCH]
}

fn validate_client(client: &str) -> Result<&'static str, AppError> {
    supported_clients()
        .into_iter()
        .find(|supported| supported.eq_ignore_ascii_case(client.trim()))
        .ok_or_else(|| AppError::Validation {
            code: "external_import.client_unsupported",
            message: "This client cannot be imported yet".to_string(),
            details: Some(client.trim().to_string()),
            recoverable: true,
        })
}

pub async fn preview_external_client_import(
    pool: &SqlitePool,
    input: PreviewExternalClientImportInput,
) -> Result<ExternalClientImportPreview, AppError> {
    let client = validate_client(&input.client)?;
    let platform = PlatformId::parse(&input.platform)?;
    PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;

    let source_path = cc_switch::resolve_source_path(input.source_path.as_deref())?;
    let providers = cc_switch::read_providers(&source_path).await?;
    let existing = existing_matches(pool, client).await?;

    let mut counts = ExternalClientImportPreviewCounts::default();
    let mut items = Vec::new();
    for provider in &providers {
        match classify(provider, platform, &existing) {
            Classification::OtherPlatform(other) => {
                counts.other_platform += 1;
                let label = other
                    .map(|platform| platform.as_str().to_string())
                    .unwrap_or_else(|| provider.app_type.trim().to_string());
                *counts.other_platform_counts.entry(label).or_default() += 1;
            }
            Classification::Item(item) => {
                counts.total += 1;
                match item.disposition.as_str() {
                    "create" => {
                        counts.create += 1;
                        counts.importable += 1;
                    }
                    "overwrite" => {
                        counts.overwrite += 1;
                        counts.importable += 1;
                    }
                    _ => counts.errors += 1,
                }
                items.push(item);
            }
        }
    }

    Ok(ExternalClientImportPreview {
        client: client.to_string(),
        source_path: source_path.display().to_string(),
        counts,
        items,
    })
}

pub async fn import_external_client_accounts(
    pool: &SqlitePool,
    input: ImportExternalClientAccountsInput,
) -> Result<ExternalClientImportOutcome, AppError> {
    let client = validate_client(&input.client)?;
    let platform = PlatformId::parse(&input.platform)?;
    PlatformCapabilityService::require(platform, PlatformOperation::RouteCredentials)?;

    let selected: HashSet<String> = input
        .source_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if selected.is_empty() {
        return Err(AppError::Validation {
            code: "external_import.selection_empty",
            message: "Select at least one account to import".to_string(),
            details: None,
            recoverable: true,
        });
    }

    // Re-read the config instead of trusting a payload from the frontend: the
    // secrets never leave this process, and a stale selection surfaces as a
    // skipped id rather than an account built from client-supplied fields.
    let source_path = cc_switch::resolve_source_path(input.source_path.as_deref())?;
    let providers = cc_switch::read_providers(&source_path).await?;
    let existing = existing_matches(pool, client).await?;

    let mut outcome = ExternalClientImportOutcome::default();
    let mut matched = HashSet::new();
    let mut tx = pool.begin().await.map_err(|error| AppError::Database {
        code: "database.external_client_import_tx",
        message: "Could not start the external account import".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;

    for provider in &providers {
        if !selected.contains(&provider.source_id) {
            continue;
        }
        matched.insert(provider.source_id.clone());

        let extracted = match cc_switch::extract_api_credential(provider) {
            Ok(extracted) if extracted.platform == platform => extracted,
            // Both arms mean the same thing to the user: the entry they picked
            // is no longer importable, so it is reported as failed rather than
            // silently dropped.
            Ok(_) | Err(_) => {
                outcome.failed += 1;
                continue;
            }
        };

        let payload = build_payload(&extracted);
        // Same guard as the preview: only overwrite a row this platform's API
        // tab could have produced.
        let matched = match overwrite_target(existing.get(&provider.source_id), platform) {
            Ok(matched) => matched,
            Err(_) => {
                outcome.failed += 1;
                continue;
            }
        };
        match matched {
            Some(existing) => {
                RouteCredentialRepository::overwrite_from_external_source(
                    &mut tx,
                    &existing.id,
                    &extracted.display_name,
                    &payload.secret_payload_json,
                    &payload.config_json,
                    &payload.preview_json,
                )
                .await?;
                outcome.overwritten += 1;
                outcome
                    .imported
                    .push(RouteCredentialRepository::get_tx(&mut tx, &existing.id).await?);
            }
            None => {
                let created = RouteCredentialRepository::create_tx_with_external_source(
                    &mut tx,
                    extracted.platform.as_str(),
                    "api",
                    &extracted.display_name,
                    None,
                    "ok",
                    None,
                    &payload.secret_payload_json,
                    &payload.config_json,
                    &payload.preview_json,
                    Some(ExternalSourceRef {
                        client,
                        source_id: &provider.source_id,
                    }),
                )
                .await?;
                outcome.created += 1;
                outcome.created_ids.push(created.id.clone());
                outcome.imported.push(created);
            }
        }
    }

    outcome.skipped = selected.len() - matched.len();

    tx.commit().await.map_err(|error| AppError::Database {
        code: "database.external_client_import_commit",
        message: "Could not save the imported accounts".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    Ok(outcome)
}

async fn existing_matches(
    pool: &SqlitePool,
    client: &str,
) -> Result<HashMap<String, ExternalSourceMatch>, AppError> {
    Ok(
        RouteCredentialRepository::external_source_matches(pool, client)
            .await?
            .into_iter()
            .collect(),
    )
}

enum Classification {
    /// Belongs to another AI Switch platform, so it is counted but not listed:
    /// the accounts page is per-platform and an entry the user cannot act on
    /// here is noise.
    OtherPlatform(Option<PlatformId>),
    Item(ExternalClientAccountPreviewItem),
}

/// The local row a re-import may overwrite, if there is one it is allowed to touch.
///
/// The unique index is global, not per platform, so the row bound to a source
/// record could in principle sit on another platform or be an official-login
/// account. Overwriting either would corrupt something the user cannot see from
/// here, so those cases become a visible error instead.
fn overwrite_target<'a>(
    matched: Option<&'a ExternalSourceMatch>,
    platform: PlatformId,
) -> Result<Option<&'a ExternalSourceMatch>, ExternalSourceMatch> {
    match matched {
        None => Ok(None),
        Some(matched)
            if matched.kind.eq_ignore_ascii_case("api")
                && PlatformId::parse(&matched.platform).is_ok_and(|local| local == platform) =>
        {
            Ok(Some(matched))
        }
        Some(matched) => Err(matched.clone()),
    }
}

const CONFLICTING_LOCAL_ACCOUNT: &str = "external_import.conflicting_local_account";

fn classify(
    provider: &ExternalClientProvider,
    platform: PlatformId,
    existing: &HashMap<String, ExternalSourceMatch>,
) -> Classification {
    let entry_platform = cc_switch::platform_for_app_type(&provider.app_type);
    match cc_switch::extract_api_credential(provider) {
        Ok(extracted) => {
            if extracted.platform != platform {
                return Classification::OtherPlatform(Some(extracted.platform));
            }
            let matched = match overwrite_target(existing.get(&provider.source_id), platform) {
                Ok(matched) => matched,
                Err(conflict) => {
                    return Classification::Item(ExternalClientAccountPreviewItem {
                        source_id: provider.source_id.clone(),
                        display_name: extracted.display_name.clone(),
                        platform: extracted.platform.as_str().to_string(),
                        interface_format: Some(extracted.interface_format.as_str().to_string()),
                        base_url: Some(extracted.base_url.clone()),
                        api_key_masked: Some(mask_api_key(&extracted.api_key)),
                        model_mapping_count: extracted.model_mappings.len(),
                        disposition: "error".to_string(),
                        existing_credential_id: Some(conflict.id),
                        existing_display_name: Some(conflict.display_name),
                        issue_codes: vec![CONFLICTING_LOCAL_ACCOUNT.to_string()],
                    })
                }
            };
            Classification::Item(ExternalClientAccountPreviewItem {
                source_id: provider.source_id.clone(),
                display_name: extracted.display_name.clone(),
                platform: extracted.platform.as_str().to_string(),
                interface_format: Some(extracted.interface_format.as_str().to_string()),
                base_url: Some(extracted.base_url.clone()),
                api_key_masked: Some(mask_api_key(&extracted.api_key)),
                model_mapping_count: extracted.model_mappings.len(),
                disposition: if matched.is_some() {
                    "overwrite"
                } else {
                    "create"
                }
                .to_string(),
                existing_credential_id: matched.map(|matched| matched.id.clone()),
                existing_display_name: matched.map(|matched| matched.display_name.clone()),
                issue_codes: Vec::new(),
            })
        }
        Err(issue) => {
            // A platform we do not map at all is not this platform's problem;
            // anything else is a real error the user should see and fix.
            if issue == ExtractIssue::PlatformUnsupported {
                return Classification::OtherPlatform(entry_platform);
            }
            if entry_platform.is_some_and(|entry_platform| entry_platform != platform) {
                return Classification::OtherPlatform(entry_platform);
            }
            Classification::Item(ExternalClientAccountPreviewItem {
                source_id: provider.source_id.clone(),
                display_name: provider.display_name.trim().to_string(),
                platform: entry_platform
                    .map(|platform| platform.as_str().to_string())
                    .unwrap_or_else(|| provider.app_type.trim().to_string()),
                interface_format: None,
                base_url: None,
                api_key_masked: None,
                model_mapping_count: 0,
                disposition: "error".to_string(),
                existing_credential_id: None,
                existing_display_name: None,
                issue_codes: vec![issue.code().to_string()],
            })
        }
    }
}

struct CredentialPayload {
    secret_payload_json: String,
    config_json: String,
    preview_json: String,
}

/// Builds the same three JSON blobs `RouteCredentialService::create_api` writes.
///
/// It deliberately mirrors that function's key set rather than calling it: the
/// import runs inside one transaction, and `create_api` owns its own connection.
/// Any key added there needs adding here too.
fn build_payload(extracted: &ExtractedApiCredential) -> CredentialPayload {
    let secret_payload_json = json!({ "api_key": extracted.api_key.trim() }).to_string();
    let mut config = json!({
        "base_url": extracted.base_url.trim(),
        "interface_format": extracted.interface_format.as_str(),
        "model_mappings": extracted.model_mappings,
        "fetched_models": Value::Array(Vec::new()),
        "responses_custom_tool_compat": false,
    });
    if let Some(api_key_field) = extracted.api_key_field {
        config["api_key_field"] = json!(api_key_field);
    }
    if let Some(user_agent) = extracted.user_agent.as_deref() {
        config["headers"] = json!({ "User-Agent": user_agent });
    }
    let config_json = config.to_string();
    let preview_json = RoutePreviewService::generate(
        extracted.platform.as_str(),
        "api",
        &secret_payload_json,
        &config_json,
    );
    CredentialPayload {
        secret_payload_json,
        config_json,
        preview_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_memory_pool, run_migrations};
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::path::Path;

    async fn write_source(path: &Path, rows: Vec<(&str, &str, &str, Value, Value)>) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let source = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("create source db");
        sqlx::query(
            "CREATE TABLE providers (
                id TEXT NOT NULL, app_type TEXT NOT NULL, name TEXT NOT NULL,
                settings_config TEXT NOT NULL, category TEXT, sort_index INTEGER,
                meta TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (id, app_type)
            )",
        )
        .execute(&source)
        .await
        .expect("create providers table");
        for (id, app_type, name, settings_config, meta) in rows {
            sqlx::query(
                "INSERT INTO providers (id, app_type, name, settings_config, category, sort_index, meta)
                 VALUES (?, ?, ?, ?, NULL, 0, ?)",
            )
            .bind(id)
            .bind(app_type)
            .bind(name)
            .bind(settings_config.to_string())
            .bind(meta.to_string())
            .execute(&source)
            .await
            .expect("insert provider");
        }
        source.close().await;
    }

    fn claude_settings(token: &str, base_url: &str) -> Value {
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": token,
                "ANTHROPIC_BASE_URL": base_url,
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-opus-5[1M]",
            }
        })
    }

    #[tokio::test]
    async fn preview_marks_new_entries_create_and_hides_other_platforms() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![
                (
                    "p1",
                    "claude",
                    "goRouter",
                    claude_settings("sk-claude-secret", "https://gorouter.app"),
                    json!({}),
                ),
                (
                    "p2",
                    "codex",
                    "kktoken",
                    json!({
                        "auth": { "OPENAI_API_KEY": "sk-codex" },
                        "config": "[model_providers.relay]\nbase_url = \"https://kktoken.cc/v1\"\n"
                    }),
                    json!({ "apiFormat": "openai_responses" }),
                ),
            ],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let preview = preview_external_client_import(
            &pool,
            PreviewExternalClientImportInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
            },
        )
        .await
        .expect("preview");

        assert_eq!(preview.counts.total, 1);
        assert_eq!(preview.counts.create, 1);
        assert_eq!(preview.counts.overwrite, 0);
        assert_eq!(preview.counts.importable, 1);
        assert_eq!(preview.counts.other_platform, 1);
        assert_eq!(preview.counts.other_platform_counts["codex"], 1);
        assert_eq!(preview.items.len(), 1);
        let item = &preview.items[0];
        assert_eq!(item.source_id, "claude:p1");
        assert_eq!(item.disposition, "create");
        assert_eq!(item.base_url.as_deref(), Some("https://gorouter.app"));
        assert_eq!(item.model_mapping_count, 1);
        assert!(item.existing_credential_id.is_none());
        // The preview is a display payload: the key is masked everywhere.
        let serialized = serde_json::to_string(&preview).expect("serialize preview");
        assert!(!serialized.contains("sk-claude-secret"));
        assert!(item
            .api_key_masked
            .as_deref()
            .is_some_and(|masked| masked.contains("***")));
    }

    #[tokio::test]
    async fn repeated_import_overwrites_the_same_account_instead_of_duplicating() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![(
                "p1",
                "claude",
                "goRouter",
                claude_settings("sk-old", "https://old.example"),
                json!({}),
            )],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let import = |path: String| async {
            import_external_client_accounts(
                &pool,
                ImportExternalClientAccountsInput {
                    client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                    platform: "claude".to_string(),
                    source_path: Some(path),
                    source_ids: vec!["claude:p1".to_string()],
                },
            )
            .await
        };

        let first = import(source_path.display().to_string())
            .await
            .expect("first import");
        assert_eq!(first.created, 1);
        assert_eq!(first.overwritten, 0);
        assert_eq!(first.imported.len(), 1);
        let credential_id = first.imported[0].id.clone();

        // The user edits the provider in cc-switch and imports again.
        tokio::fs::remove_file(&source_path)
            .await
            .expect("replace source");
        write_source(
            &source_path,
            vec![(
                "p1",
                "claude",
                "goRouter renamed",
                claude_settings("sk-new", "https://new.example"),
                json!({}),
            )],
        )
        .await;
        let second = import(source_path.display().to_string())
            .await
            .expect("second import");
        assert_eq!(second.created, 0);
        assert_eq!(second.overwritten, 1);
        assert_eq!(second.imported[0].id, credential_id);
        assert_eq!(second.imported[0].display_name, "goRouter renamed");
        assert!(second.imported[0]
            .config_json
            .contains("https://new.example"));
        assert!(second.imported[0].secret_payload_json.contains("sk-new"));

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM route_credentials")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(total, 1, "a re-import must not add a second row");

        let preview = preview_external_client_import(
            &pool,
            PreviewExternalClientImportInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
            },
        )
        .await
        .expect("preview after import");
        assert_eq!(preview.counts.overwrite, 1);
        assert_eq!(preview.counts.create, 0);
        assert_eq!(
            preview.items[0].existing_credential_id.as_deref(),
            Some(credential_id.as_str())
        );
        assert_eq!(
            preview.items[0].existing_display_name.as_deref(),
            Some("goRouter renamed")
        );
    }

    #[tokio::test]
    async fn import_writes_the_same_config_keys_as_a_hand_made_api_account() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![(
                "p1",
                "claude",
                "Any",
                json!({
                    "env": {
                        "ANTHROPIC_API_KEY": "sk-any",
                        "ANTHROPIC_BASE_URL": "https://anyrouter.top",
                    }
                }),
                json!({ "customUserAgent": "claude-cli/2.1.161 (external, cli)" }),
            )],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let outcome = import_external_client_accounts(
            &pool,
            ImportExternalClientAccountsInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
                source_ids: vec!["claude:p1".to_string()],
            },
        )
        .await
        .expect("import");

        let config: Value =
            serde_json::from_str(&outcome.imported[0].config_json).expect("config json");
        assert_eq!(config["base_url"], "https://anyrouter.top");
        assert_eq!(config["interface_format"], "anthropic");
        assert_eq!(config["api_key_field"], "ANTHROPIC_API_KEY");
        assert_eq!(config["model_mappings"], json!([]));
        assert_eq!(config["fetched_models"], json!([]));
        assert_eq!(config["responses_custom_tool_compat"], false);
        assert_eq!(
            config["headers"]["User-Agent"],
            "claude-cli/2.1.161 (external, cli)"
        );
        assert_eq!(outcome.imported[0].kind, "api");
        assert_eq!(outcome.imported[0].status, "ok");
        assert_ne!(outcome.imported[0].preview_json, "{}");
    }

    #[tokio::test]
    async fn unselected_and_vanished_entries_are_skipped_not_imported() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![
                (
                    "p1",
                    "claude",
                    "Picked",
                    claude_settings("sk-one", "https://one.example"),
                    json!({}),
                ),
                (
                    "p2",
                    "claude",
                    "Not picked",
                    claude_settings("sk-two", "https://two.example"),
                    json!({}),
                ),
            ],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let outcome = import_external_client_accounts(
            &pool,
            ImportExternalClientAccountsInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
                // One real id plus one the config no longer has.
                source_ids: vec!["claude:p1".to_string(), "claude:gone".to_string()],
            },
        )
        .await
        .expect("import");

        assert_eq!(outcome.created, 1);
        assert_eq!(outcome.skipped, 1);
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.imported[0].display_name, "Picked");
    }

    #[tokio::test]
    async fn broken_entries_are_previewed_as_errors_and_refused_on_import() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![(
                "p1",
                "claude",
                "No key",
                json!({ "env": { "ANTHROPIC_BASE_URL": "https://relay.example" } }),
                json!({}),
            )],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let preview = preview_external_client_import(
            &pool,
            PreviewExternalClientImportInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
            },
        )
        .await
        .expect("preview");
        assert_eq!(preview.counts.errors, 1);
        assert_eq!(preview.counts.importable, 0);
        assert_eq!(preview.items[0].disposition, "error");
        assert_eq!(
            preview.items[0].issue_codes,
            vec!["external_import.api_key_missing".to_string()]
        );

        let outcome = import_external_client_accounts(
            &pool,
            ImportExternalClientAccountsInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
                source_ids: vec!["claude:p1".to_string()],
            },
        )
        .await
        .expect("import");
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.created, 0);
        assert!(outcome.imported.is_empty());
    }

    #[tokio::test]
    async fn rejects_unknown_clients_and_empty_selections() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");

        let unknown = preview_external_client_import(
            &pool,
            PreviewExternalClientImportInput {
                client: "some-other-app".to_string(),
                platform: "claude".to_string(),
                source_path: None,
            },
        )
        .await
        .expect_err("unknown client");
        assert!(matches!(
            unknown,
            AppError::Validation {
                code: "external_import.client_unsupported",
                ..
            }
        ));

        let empty = import_external_client_accounts(
            &pool,
            ImportExternalClientAccountsInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: None,
                source_ids: Vec::new(),
            },
        )
        .await
        .expect_err("empty selection");
        assert!(matches!(
            empty,
            AppError::Validation {
                code: "external_import.selection_empty",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn refuses_to_overwrite_a_local_row_this_platform_does_not_own() {
        // The unique index is global, so a source record could be bound to a row
        // on another platform (e.g. the provider was imported under Codex, then
        // re-typed as Claude in cc-switch). Overwriting it would silently rewrite
        // an account the user cannot see from this tab.
        let dir = tempfile::tempdir().expect("temp dir");
        let source_path = dir.path().join("cc-switch.db");
        write_source(
            &source_path,
            vec![(
                "p1",
                "claude",
                "Moved provider",
                claude_settings("sk-one", "https://one.example"),
                json!({}),
            )],
        )
        .await;

        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let mut tx = pool.begin().await.expect("transaction");
        let foreign = RouteCredentialRepository::create_tx_with_external_source(
            &mut tx,
            "codex",
            "api",
            "Imported as Codex",
            None,
            "ok",
            None,
            r#"{"api_key":"sk-codex"}"#,
            "{}",
            "{}",
            Some(ExternalSourceRef {
                client: EXTERNAL_CLIENT_CC_SWITCH,
                source_id: "claude:p1",
            }),
        )
        .await
        .expect("seed foreign-platform row");
        tx.commit().await.expect("commit");

        let preview = preview_external_client_import(
            &pool,
            PreviewExternalClientImportInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
            },
        )
        .await
        .expect("preview");
        assert_eq!(preview.counts.errors, 1);
        assert_eq!(preview.counts.importable, 0);
        assert_eq!(preview.items[0].disposition, "error");
        assert_eq!(
            preview.items[0].issue_codes,
            vec!["external_import.conflicting_local_account".to_string()]
        );

        let outcome = import_external_client_accounts(
            &pool,
            ImportExternalClientAccountsInput {
                client: EXTERNAL_CLIENT_CC_SWITCH.to_string(),
                platform: "claude".to_string(),
                source_path: Some(source_path.display().to_string()),
                source_ids: vec!["claude:p1".to_string()],
            },
        )
        .await
        .expect("import");
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.created, 0);
        assert_eq!(outcome.overwritten, 0);
        // The Codex row is untouched.
        let untouched = RouteCredentialRepository::get(&pool, &foreign.id)
            .await
            .expect("foreign row still there");
        assert_eq!(untouched.display_name, "Imported as Codex");
        assert!(untouched.secret_payload_json.contains("sk-codex"));
    }

    #[test]
    fn masking_never_reveals_a_usable_key() {
        assert_eq!(mask_api_key("sk-1234567890"), "sk-1***7890");
        assert_eq!(mask_api_key("short"), "sh***");
        assert_eq!(mask_api_key("   "), "(empty)");
    }
}
