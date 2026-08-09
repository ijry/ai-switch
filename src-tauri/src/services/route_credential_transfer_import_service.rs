#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::batch_repository::BatchRepository;
    use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
    use crate::database::repositories::route_credential_transfer_repository::{
        insert_origin_tx, TransferOrigin,
    };
    use crate::database::repositories::route_pool_repository::RoutePoolRepository;
    use crate::database::{create_memory_pool, run_migrations};
    use crate::error::{ApiError, AppError};
    use crate::models::batch::NewBatch;
    use crate::models::route_credential_transfer::{
        ImportRouteCredentialsInput, PreviewRouteCredentialImportInput, TransferPlatformChoice,
        TRANSFER_FORMAT, TRANSFER_MAX_BYTES, TRANSFER_MAX_ITEMS, TRANSFER_MAX_ITEM_BYTES,
    };
    use crate::services::cpa_export_service::trusted_cpa_raw_template;
    use serde_json::{json, Map, Value};
    use sqlx::SqlitePool;

    fn endpoint() -> String {
        ["https://", "api.example.invalid", "/v1"].concat()
    }

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(object) => object,
            _ => panic!("fixture must be an object"),
        }
    }

    fn api_item(section: &str, platform: &str, interface_format: &str) -> Map<String, Value> {
        object(json!({
            "api-key": "fixture-key-material",
            "base-url": endpoint(),
            "x-ai-switch": {
                "format": TRANSFER_FORMAT,
                "schema_version": 1,
                "platform": platform,
                "kind": "api",
                "cpa_section": section,
                "display_name": "Portable route",
                "interface_format": interface_format
            }
        }))
    }

    fn official_item() -> Map<String, Value> {
        object(json!({
            "type": "codex",
            "email": "fixture@example.invalid",
            "access_token": "fixture-access-material",
            "x-ai-switch": {
                "format": TRANSFER_FORMAT,
                "schema_version": 1,
                "platform": "codex",
                "kind": "official",
                "display_name": "Official fixture"
            }
        }))
    }

    fn compatibility_item(platform: &str) -> Map<String, Value> {
        object(json!({
            "name": "Compatibility fixture",
            "api-key-entries": [{"api-key": "fixture-key-material"}],
            "base-url": endpoint(),
            "x-ai-switch": {
                "format": TRANSFER_FORMAT,
                "schema_version": 1,
                "platform": platform,
                "kind": "api",
                "cpa_section": "openai-compatibility",
                "display_name": "Compatibility fixture",
                "interface_format": "openai"
            }
        }))
    }

    fn metadata_mut(item: &mut Map<String, Value>) -> &mut Map<String, Value> {
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
    }

    fn set_source_identity(
        item: &mut Map<String, Value>,
        source_instance_id: &str,
        source_credential_id: &str,
    ) {
        let metadata = metadata_mut(item);
        metadata.insert("source_instance_id".to_string(), json!(source_instance_id));
        metadata.insert(
            "source_credential_id".to_string(),
            json!(source_credential_id),
        );
    }

    fn set_batch(item: &mut Map<String, Value>, source_batch_id: Option<&str>, batch_name: &str) {
        let metadata = metadata_mut(item);
        if let Some(source_batch_id) = source_batch_id {
            metadata.insert("source_batch_id".to_string(), json!(source_batch_id));
        }
        metadata.insert("batch_name".to_string(), json!(batch_name));
    }

    fn set_in_pool(item: &mut Map<String, Value>) {
        metadata_mut(item).insert("in_pool".to_string(), json!(true));
    }

    async fn migrated_pool() -> SqlitePool {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        pool
    }

    async fn table_count(pool: &SqlitePool, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .expect("table count")
    }

    fn import_input(
        items: Vec<Map<String, Value>>,
        restore_pool_membership: bool,
    ) -> ImportRouteCredentialsInput {
        ImportRouteCredentialsInput {
            text: Value::Array(items.into_iter().map(Value::Object).collect()).to_string(),
            ambiguous_platform_choices: Vec::new(),
            restore_pool_membership,
        }
    }

    async fn insert_origin(
        pool: &SqlitePool,
        item: &NormalizedImportItem,
        source_fingerprint: &str,
    ) {
        let credential = RouteCredentialRepository::create(
            pool,
            &item.platform,
            &item.kind,
            "Existing origin",
            None,
            "ok",
            None,
            &item.secret_payload_json,
            &item.config_json,
            &item.preview_json,
        )
        .await
        .expect("credential");
        let identity = item.source_identity.as_ref().expect("source identity");
        let mut tx = pool.begin().await.expect("transaction");
        insert_origin_tx(
            &mut tx,
            &TransferOrigin {
                route_credential_id: credential.id,
                source_instance_id: identity.source_instance_id.clone(),
                source_credential_id: identity.source_credential_id.clone(),
                source_platform: identity.source_platform.clone(),
                source_kind: identity.source_kind.clone(),
                source_schema_version: identity.source_schema_version,
                source_fingerprint: source_fingerprint.to_string(),
                imported_at: "2026-08-05T00:00:00Z".to_string(),
            },
        )
        .await
        .expect("origin");
        tx.commit().await.expect("commit origin");
    }

    fn choice(item_index: usize, platform: &str, interface_format: &str) -> TransferPlatformChoice {
        TransferPlatformChoice {
            item_index,
            platform: platform.to_string(),
            interface_format: Some(interface_format.to_string()),
        }
    }

    fn app_error_code(error: &AppError) -> &'static str {
        match error {
            AppError::Validation { code, .. }
            | AppError::Filesystem { code, .. }
            | AppError::Database { code, .. }
            | AppError::Secret { code, .. } => code,
        }
    }

    fn classify_ok(
        item_index: usize,
        item: &Map<String, Value>,
        choices: &[TransferPlatformChoice],
    ) -> NormalizedImportItem {
        match classify_transfer_item(item_index, item, choices) {
            Ok(item) => item,
            Err(issue) => panic!("unexpected classification issue: {}", issue.code),
        }
    }

    fn classify_issue(
        item_index: usize,
        item: &Map<String, Value>,
        choices: &[TransferPlatformChoice],
    ) -> crate::models::route_credential_transfer::RouteCredentialTransferIssue {
        match classify_transfer_item(item_index, item, choices) {
            Ok(_) => panic!("expected classification issue"),
            Err(issue) => issue,
        }
    }

    #[test]
    fn parser_rejects_non_array_roots_without_echoing_input() {
        let source_marker = "fixture-source-marker";
        let text = format!(r#"{{"access_token":"{source_marker}"}}"#);
        let error = match validate_transfer_text(&text) {
            Ok(_) => panic!("expected array validation error"),
            Err(error) => error,
        };

        assert_eq!(app_error_code(&error), "validation.transfer_array_required");
        let serialized = serde_json::to_string(&ApiError::from(error)).expect("serialize error");
        assert!(!serialized.contains(source_marker));
    }

    #[test]
    fn parser_rejects_non_object_entries_and_invalid_json_safely() {
        let non_object = match validate_transfer_text("[null]") {
            Ok(_) => panic!("expected object validation error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&non_object),
            "validation.transfer_item_object_required"
        );

        let source_marker = "fixture-invalid-source-marker";
        let invalid_text = format!(r#"[{{"refresh_token":"{source_marker}"}}"#);
        let invalid = match validate_transfer_text(&invalid_text) {
            Ok(_) => panic!("expected JSON validation error"),
            Err(error) => error,
        };
        let serialized = serde_json::to_string(&ApiError::from(invalid)).expect("serialize error");
        assert!(!serialized.contains(source_marker));
    }

    #[test]
    fn parser_enforces_text_item_count_and_compact_item_limits() {
        let oversized_text = " ".repeat(TRANSFER_MAX_BYTES + 1);
        let text_error = match validate_transfer_text(&oversized_text) {
            Ok(_) => panic!("expected text limit error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&text_error),
            "validation.transfer_text_too_large"
        );

        let too_many = serde_json::to_string(&vec![json!({}); TRANSFER_MAX_ITEMS + 1])
            .expect("serialize fixture");
        let count_error = match validate_transfer_text(&too_many) {
            Ok(_) => panic!("expected item count error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&count_error),
            "validation.transfer_item_limit"
        );

        let oversized_item = json!([{"padding": "x".repeat(TRANSFER_MAX_ITEM_BYTES)}]).to_string();
        let item_error = match validate_transfer_text(&oversized_item) {
            Ok(_) => panic!("expected item size error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&item_error),
            "validation.transfer_item_too_large"
        );
    }

    #[test]
    fn batch_normalization_rejects_duplicate_missing_and_unused_choices() {
        let text = Value::Array(vec![Value::Object(object(json!({
            "api-key": "fixture-key-material",
            "base-url": endpoint()
        })))])
        .to_string();

        let duplicate = match normalize_transfer_items(
            &text,
            &[
                choice(0, "claude", "anthropic"),
                choice(0, "claude", "anthropic"),
            ],
        ) {
            Ok(_) => panic!("expected duplicate choice error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&duplicate),
            "validation.transfer_choice_duplicate"
        );

        let missing = match normalize_transfer_items(&text, &[choice(1, "claude", "anthropic")]) {
            Ok(_) => panic!("expected missing choice target error"),
            Err(error) => error,
        };
        assert_eq!(
            app_error_code(&missing),
            "validation.transfer_choice_target_missing"
        );

        let official_text = Value::Array(vec![Value::Object(official_item())]).to_string();
        let unused = match normalize_transfer_items(
            &official_text,
            &[choice(0, "codex", "openai-responses")],
        ) {
            Ok(_) => panic!("expected unused choice error"),
            Err(error) => error,
        };
        assert_eq!(app_error_code(&unused), "validation.transfer_choice_unused");
    }

    #[test]
    fn mixed_official_and_api_arrays_normalize_independently() {
        let text = Value::Array(vec![
            Value::Object(official_item()),
            Value::Object(api_item("codex-api-key", "codex", "openai-responses")),
        ])
        .to_string();
        let normalized = match normalize_transfer_items(&text, &[]) {
            Ok(items) => items,
            Err(error) => panic!("unexpected parser error: {}", app_error_code(&error)),
        };

        assert_eq!(normalized.len(), 2);
        let first = match &normalized[0] {
            Ok(item) => item,
            Err(issue) => panic!("unexpected issue: {}", issue.code),
        };
        let second = match &normalized[1] {
            Ok(item) => item,
            Err(issue) => panic!("unexpected issue: {}", issue.code),
        };
        assert_eq!(first.kind, "official");
        assert_eq!(first.platform, "codex");
        assert_eq!(second.kind, "api");
        assert_eq!(second.cpa_section.as_deref(), Some("codex-api-key"));
    }

    #[test]
    fn metadata_free_api_requires_and_validates_structured_choice() {
        let item = object(json!({
            "api-key": "fixture-key-material",
            "base-url": endpoint()
        }));
        let missing = classify_issue(0, &item, &[]);
        assert_eq!(missing.code, "transfer.choice_required");

        let normalized = classify_ok(0, &item, &[choice(0, "claude", "anthropic")]);
        assert_eq!(normalized.platform, "claude");
        assert_eq!(normalized.cpa_section.as_deref(), Some("claude-api-key"));

        let incompatible = object(json!({
            "type": "codex-api-key",
            "api-key": "fixture-key-material",
            "base-url": endpoint()
        }));
        let issue = classify_issue(0, &incompatible, &[choice(0, "claude", "anthropic")]);
        assert_eq!(issue.code, "transfer.choice_incompatible");
    }

    #[test]
    fn unsupported_sections_top_level_api_keys_and_unknown_major_are_rejected() {
        for section in ["interactions-api-key", "vertex-api-key"] {
            let mut item = api_item(section, "codex", "openai");
            let issue = classify_issue(0, &item, &[]);
            assert_eq!(issue.code, "transfer.cpa_section_unsupported");
            item.clear();
        }

        let top_level = object(json!({"api-keys": ["fixture-key-material"]}));
        assert_eq!(
            classify_issue(0, &top_level, &[]).code,
            "transfer.top_level_api_keys_unsupported"
        );

        let mut unknown_major = api_item("codex-api-key", "codex", "openai-responses");
        unknown_major
            .get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("schema_version".to_string(), json!(2));
        assert_eq!(
            classify_issue(0, &unknown_major, &[]).code,
            "transfer.schema_version_unsupported"
        );
    }

    #[test]
    fn metadata_and_payload_semantic_conflicts_are_fatal() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("kind".to_string(), json!("official"));
        assert_eq!(
            classify_issue(0, &item, &[]).code,
            "transfer.metadata_conflict"
        );

        let contradictory = api_item("claude-api-key", "claude", "gemini");
        assert_eq!(
            classify_issue(0, &contradictory, &[]).code,
            "transfer.interface_format_conflict"
        );
    }

    #[test]
    fn agent_identity_never_falls_back_to_oauth_requirements() {
        let incomplete = object(json!({
            "type": "codex",
            "access_token": "fixture-access-material",
            "auth_mode": "agentIdentity",
            "agent_private_key": "fixture-private-material",
            "agent_runtime_id": "runtime-fixture",
            "task_id": "task-fixture"
        }));
        let issue = classify_issue(0, &incomplete, &[]);
        assert_eq!(issue.code, "transfer.agent_identity_field_required");
        assert_eq!(issue.field.as_deref(), Some("account_id"));

        let complete = object(json!({
            "type": "codex",
            "auth_mode": "agentIdentity",
            "agent_private_key": "fixture-private-material",
            "agent_runtime_id": "runtime-fixture",
            "task_id": "task-fixture",
            "workspace_id": "workspace-fixture"
        }));
        assert_eq!(classify_ok(0, &complete, &[]).kind, "official");
    }

    #[test]
    fn official_normalization_preserves_client_id_and_builds_trusted_raw() {
        let item = object(json!({
            "type": "codex",
            "refresh_token": "fixture-refresh-material",
            "client_id": "fixture-client-identity",
            "future_auth_field": {"enabled": true},
            "raw": {"access_token": "untrusted-nested-material"},
            "import_format": "untrusted-claim"
        }));
        let normalized = classify_ok(0, &item, &[]);
        let secret: Value = serde_json::from_str(&normalized.secret_payload_json).expect("secret");
        let config: Value = serde_json::from_str(&normalized.config_json).expect("config");

        assert!(secret.get("client_id").and_then(Value::as_str).is_some());
        assert_eq!(
            config.get("import_format").and_then(Value::as_str),
            Some("auth-file")
        );
        assert!(config
            .get("raw")
            .and_then(Value::as_object)
            .is_some_and(|raw| raw.contains_key("future_auth_field")
                && !raw.contains_key("raw")
                && !raw.contains_key("import_format")));
        assert!(trusted_cpa_raw_template("codex", &config));
    }

    #[test]
    fn reverse_mapping_restores_cpa_models_and_compatible_metadata() {
        let item = object(json!({
            "api-key": "fixture-key-material",
            "base-url": endpoint(),
            "models": [{
                "name": "provider-model",
                "alias": "local-model",
                "display-name": "Display Model",
                "max-context-length": 1048576
            }],
            "x-ai-switch": {
                "format": TRANSFER_FORMAT,
                "schema_version": 1,
                "platform": "codex",
                "kind": "api",
                "cpa_section": "codex-api-key",
                "interface_format": "openai-responses",
                "responses_custom_tool_compat": true,
                "model_mappings": [{
                    "from": "local-model",
                    "to": "provider-model",
                    "label": "Display Model",
                    "supports_1m": true
                }]
            }
        }));
        let normalized = classify_ok(0, &item, &[]);
        let config: Value = serde_json::from_str(&normalized.config_json).expect("config");
        let mappings = config
            .get("model_mappings")
            .and_then(Value::as_array)
            .expect("model mappings");

        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings[0].get("from").and_then(Value::as_str),
            Some("local-model")
        );
        assert_eq!(
            mappings[0].get("to").and_then(Value::as_str),
            Some("provider-model")
        );
        assert_eq!(
            mappings[0].get("label").and_then(Value::as_str),
            Some("Display Model")
        );
        assert_eq!(
            mappings[0].get("supports_1m").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            config
                .get("responses_custom_tool_compat")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn model_metadata_must_match_the_projected_cpa_models() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.insert(
            "models".to_string(),
            json!([{"name": "provider-a", "alias": "local-a"}]),
        );
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert(
                "model_mappings".to_string(),
                json!([{"from": "local-b", "to": "provider-b"}]),
            );

        assert_eq!(
            classify_issue(0, &item, &[]).code,
            "transfer.model_mappings_conflict"
        );
    }

    #[test]
    fn same_major_optional_fields_and_api_non_secret_fields_are_warnings() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.insert("future-routing-option".to_string(), json!(true));
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("future_optional_field".to_string(), json!(true));
        let normalized = classify_ok(0, &item, &[]);

        assert!(normalized
            .issue_codes
            .iter()
            .any(|code| code == "transfer.api_field_ignored"));
        assert!(normalized
            .issue_codes
            .iter()
            .any(|code| code == "transfer.metadata_field_ignored"));

        let mut secretish = api_item("codex-api-key", "codex", "openai-responses");
        secretish.insert("vendor-token".to_string(), json!("fixture-secret-material"));
        assert_eq!(
            classify_issue(0, &secretish, &[]).code,
            "transfer.api_secret_field_unsupported"
        );
    }

    #[test]
    fn legacy_api_type_is_only_a_discriminator_and_is_not_persisted() {
        let item = object(json!({
            "type": "openai-compatibility",
            "name": "Legacy compatible route",
            "base-url": endpoint(),
            "api-key-entries": [{"api-key": "fixture-key-material"}]
        }));
        let normalized = classify_ok(0, &item, &[choice(0, "codex", "openai")]);
        let config: Value = serde_json::from_str(&normalized.config_json).expect("config");

        assert_eq!(
            normalized.legacy_type.as_deref(),
            Some("openai-compatibility")
        );
        assert!(config.get("type").is_none());
        assert_eq!(
            normalized.cpa_section.as_deref(),
            Some("openai-compatibility")
        );
    }

    #[test]
    fn openai_compatibility_requires_exactly_one_key_entry() {
        let item = object(json!({
            "name": "Compatibility route",
            "base-url": endpoint(),
            "api-key-entries": [
                {"api-key": "fixture-key-material-a"},
                {"api-key": "fixture-key-material-b"}
            ],
            "x-ai-switch": {
                "format": TRANSFER_FORMAT,
                "schema_version": 1,
                "platform": "codex",
                "kind": "api",
                "cpa_section": "openai-compatibility"
            }
        }));

        assert_eq!(
            classify_issue(0, &item, &[]).code,
            "transfer.api_key_entries_count"
        );
    }

    #[test]
    fn partial_source_identity_warns_without_creating_global_identity() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("source_credential_id".to_string(), json!("source-fixture"));
        let normalized = classify_ok(0, &item, &[]);

        assert!(normalized.source_identity.is_none());
        assert!(normalized
            .issue_codes
            .iter()
            .any(|code| code == "transfer.source_identity_partial"));
    }

    #[test]
    fn display_names_are_trimmed_masked_and_preview_is_regenerated() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.insert("preview_json".to_string(), json!("incoming-preview"));
        let metadata = item
            .get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata");
        metadata.insert("display_name".to_string(), json!("  路由甲  "));
        let normalized = classify_ok(0, &item, &[]);

        assert_eq!(normalized.display_name, "路由甲");
        assert_eq!(normalized.display_name_masked, "路***甲");
        assert_ne!(normalized.preview_json, "incoming-preview");

        let mut empty = api_item("codex-api-key", "codex", "openai-responses");
        empty
            .get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("display_name".to_string(), json!("  "));
        let normalized = classify_ok(3, &empty, &[]);
        assert_eq!(normalized.display_name, "Item 4");
        assert_eq!(normalized.display_name_masked, "I***4");
    }

    #[test]
    fn untrusted_secret_field_names_are_not_returned_in_issues() {
        let secret_field = "fixture-token-field-marker";
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.insert(secret_field.to_string(), json!("fixture-secret-material"));

        let issue = classify_issue(0, &item, &[]);

        assert_eq!(issue.code, "transfer.api_secret_field_unsupported");
        assert_ne!(issue.field.as_deref(), Some(secret_field));
        assert!(!serde_json::to_string(&issue)
            .expect("serialize issue")
            .contains(secret_field));
    }

    #[test]
    fn shape_only_choice_rejects_fixed_section_platform_mismatch() {
        let item = object(json!({
            "api-key": "fixture-key-material",
            "base-url": endpoint()
        }));

        let issue = classify_issue(0, &item, &[choice(0, "codex", "anthropic")]);

        assert_eq!(issue.code, "transfer.choice_incompatible");
    }

    #[test]
    fn ambiguous_choice_remains_consumed_when_payload_is_invalid() {
        let text = json!([{"base-url": endpoint()}]).to_string();
        let normalized = normalize_transfer_items(&text, &[choice(0, "claude", "anthropic")])
            .expect("choice should remain attached to the ambiguous item");

        assert_eq!(normalized.len(), 1);
        let issue = match &normalized[0] {
            Err(issue) => issue,
            Ok(_) => panic!("missing key must fail"),
        };
        assert_eq!(issue.code, "transfer.api_key_required");
    }

    #[test]
    fn nested_unknown_secret_fields_are_rejected_and_model_metadata_is_validated() {
        let mut metadata_secret = api_item("codex-api-key", "codex", "openai-responses");
        metadata_secret
            .get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert(
                "future_auth".to_string(),
                json!({"vendor-token": "fixture-secret-material"}),
            );
        let issue = classify_issue(0, &metadata_secret, &[]);
        assert_eq!(issue.code, "transfer.metadata_secret_field_unsupported");
        assert_eq!(issue.field.as_deref(), Some("unknown_secret_field"));

        let mut model_secret = api_item("codex-api-key", "codex", "openai-responses");
        model_secret.insert(
            "models".to_string(),
            json!([{
                "name": "provider-model",
                "alias": "local-model",
                "vendor-token": "fixture-secret-material"
            }]),
        );
        let issue = classify_issue(0, &model_secret, &[]);
        assert_eq!(issue.code, "transfer.api_secret_field_unsupported");
        assert_eq!(issue.field.as_deref(), Some("unknown_secret_field"));

        let mut invalid_metadata = api_item("codex-api-key", "codex", "openai-responses");
        invalid_metadata.insert(
            "models".to_string(),
            json!([{"name": "provider-model", "alias": "local-model"}]),
        );
        invalid_metadata
            .get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert(
                "model_mappings".to_string(),
                json!([{"to": "provider-model"}]),
            );
        assert_eq!(
            classify_issue(0, &invalid_metadata, &[]).code,
            "transfer.model_mappings_conflict"
        );
    }

    #[test]
    fn known_metadata_fields_with_nested_secrets_are_rejected_safely() {
        for (field, value) in [
            (
                "origin_format",
                json!({"access_token": "fixture-secret-material"}),
            ),
            (
                "source_instance_id",
                json!([{"vendor-token": "fixture-secret-material"}]),
            ),
            (
                "in_pool",
                json!([{"refresh_token": "fixture-secret-material"}]),
            ),
            (
                "schema_version",
                json!({"private-key": "fixture-secret-material"}),
            ),
        ] {
            let mut item = api_item("codex-api-key", "codex", "openai-responses");
            item.get_mut("x-ai-switch")
                .and_then(Value::as_object_mut)
                .expect("metadata")
                .insert(field.to_string(), value);

            let issue = classify_issue(0, &item, &[]);
            assert_eq!(issue.code, "transfer.metadata_secret_field_unsupported");
            assert_eq!(issue.field.as_deref(), Some("unknown_secret_field"));
            assert_ne!(issue.field.as_deref(), Some(field));
        }
    }

    #[test]
    fn known_optional_metadata_fields_with_wrong_non_secret_types_warn() {
        let mut item = api_item("codex-api-key", "codex", "openai-responses");
        item.get_mut("x-ai-switch")
            .and_then(Value::as_object_mut)
            .expect("metadata")
            .insert("origin_format".to_string(), json!({"future": true}));

        let normalized = classify_ok(0, &item, &[]);
        assert!(normalized
            .issue_codes
            .iter()
            .any(|code| code == "transfer.metadata_field_ignored"));
    }

    #[tokio::test]
    async fn preview_route_credential_import_prioritizes_input_duplicates_and_checks_origins() {
        let pool = migrated_pool().await;
        let mut conflict = api_item("codex-api-key", "codex", "openai-responses");
        conflict.insert("api-key".to_string(), json!("conflict-key-material"));
        set_source_identity(&mut conflict, "source-instance-a", "source-credential-a");
        set_batch(&mut conflict, Some("conflict-batch"), "Conflict batch");
        set_in_pool(&mut conflict);
        let conflict_normalized = classify_ok(0, &conflict, &[]);
        insert_origin(&pool, &conflict_normalized, "different-source-fingerprint").await;

        let mut input_duplicate = conflict.clone();
        set_source_identity(
            &mut input_duplicate,
            "source-instance-b",
            "source-credential-b",
        );
        set_batch(
            &mut input_duplicate,
            Some("input-duplicate-batch"),
            "Input duplicate batch",
        );

        let mut source_duplicate = api_item("codex-api-key", "codex", "openai-responses");
        source_duplicate.insert("api-key".to_string(), json!("duplicate-key-material"));
        set_source_identity(
            &mut source_duplicate,
            "source-instance-c",
            "source-credential-c",
        );
        set_batch(
            &mut source_duplicate,
            Some("source-duplicate-batch"),
            "Source duplicate batch",
        );
        set_in_pool(&mut source_duplicate);
        let source_duplicate_normalized = classify_ok(2, &source_duplicate, &[]);
        insert_origin(
            &pool,
            &source_duplicate_normalized,
            &source_duplicate_normalized.fingerprint,
        )
        .await;

        let preview = preview_route_credential_import(
            &pool,
            PreviewRouteCredentialImportInput {
                text: Value::Array(vec![
                    Value::Object(conflict),
                    Value::Object(input_duplicate),
                    Value::Object(source_duplicate),
                ])
                .to_string(),
                ambiguous_platform_choices: Vec::new(),
            },
        )
        .await
        .expect("preview");

        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| item.disposition.as_str())
                .collect::<Vec<_>>(),
            vec!["conflict", "input_duplicate", "source_duplicate"]
        );
        assert_eq!(preview.counts.importable, 0);
        assert_eq!(preview.counts.duplicates, 2);
        assert_eq!(preview.counts.conflicts, 1);
        assert_eq!(preview.counts.errors, 0);
        assert_eq!(preview.counts.batch_count, 0);
        assert_eq!(preview.counts.restorable_pool_count, 0);
        assert!(preview.counts.restorable_pool_counts.is_empty());
    }

    #[tokio::test]
    async fn preview_route_credential_import_tracks_only_imported_source_identities() {
        let pool = migrated_pool().await;

        let mut first = api_item("codex-api-key", "codex", "openai-responses");
        first.insert("api-key".to_string(), json!("identity-first-key"));
        set_source_identity(&mut first, "same-source-instance", "same-source-credential");

        let mut conflicting = first.clone();
        conflicting.insert("api-key".to_string(), json!("identity-conflicting-key"));

        let mut identityless = api_item("codex-api-key", "codex", "openai-responses");
        identityless.insert("api-key".to_string(), json!("identityless-duplicate-key"));

        let mut skipped_duplicate = identityless.clone();
        set_source_identity(
            &mut skipped_duplicate,
            "later-source-instance",
            "later-source-credential",
        );

        let mut later_import = skipped_duplicate.clone();
        later_import.insert("api-key".to_string(), json!("later-import-key"));

        let preview = preview_route_credential_import(
            &pool,
            PreviewRouteCredentialImportInput {
                text: Value::Array(vec![
                    Value::Object(first),
                    Value::Object(conflicting),
                    Value::Object(identityless),
                    Value::Object(skipped_duplicate),
                    Value::Object(later_import),
                ])
                .to_string(),
                ambiguous_platform_choices: Vec::new(),
            },
        )
        .await
        .expect("preview");

        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| item.disposition.as_str())
                .collect::<Vec<_>>(),
            vec!["import", "conflict", "import", "input_duplicate", "import"]
        );
        assert_eq!(preview.counts.importable, 3);
        assert_eq!(preview.counts.duplicates, 1);
        assert_eq!(preview.counts.conflicts, 1);
    }

    #[tokio::test]
    async fn preview_route_credential_import_deduplicates_fingerprints_across_platforms() {
        let pool = migrated_pool().await;

        let preview = preview_route_credential_import(
            &pool,
            PreviewRouteCredentialImportInput {
                text: Value::Array(vec![
                    Value::Object(compatibility_item("codex")),
                    Value::Object(compatibility_item("claude")),
                ])
                .to_string(),
                ambiguous_platform_choices: Vec::new(),
            },
        )
        .await
        .expect("preview");

        assert_eq!(preview.items[0].disposition, "import");
        assert_eq!(preview.items[1].disposition, "input_duplicate");
        assert_eq!(preview.counts.importable, 1);
        assert_eq!(preview.counts.duplicates, 1);
    }

    #[tokio::test]
    async fn preview_route_credential_import_detects_possible_duplicates_and_ignores_bad_candidates(
    ) {
        let pool = migrated_pool().await;
        RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Existing compatibility route",
            None,
            "ok",
            None,
            r#"{"api_key":"fixture-key-material"}"#,
            &json!({
                "base_url": endpoint(),
                "interface_format": "openai",
                "model_mappings": []
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("valid candidate");
        RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Existing trusted-source route",
            None,
            "ok",
            None,
            r#"{"api_key":"trusted-source-key-material"}"#,
            &json!({
                "base_url": endpoint(),
                "interface_format": "openai",
                "model_mappings": []
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("trusted-source candidate");
        RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Existing fixed-section route",
            None,
            "ok",
            None,
            r#"{"api_key":"fixed-section-key-material"}"#,
            &json!({
                "base_url": endpoint(),
                "interface_format": "openai-responses",
                "model_mappings": []
            })
            .to_string(),
            "{}",
        )
        .await
        .expect("fixed-section candidate");
        RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Unprojectable route",
            None,
            "ok",
            None,
            "{}",
            "{}",
            "{}",
        )
        .await
        .expect("invalid candidate fixture");

        let mut trusted_source = compatibility_item("codex");
        trusted_source
            .get_mut("api-key-entries")
            .and_then(Value::as_array_mut)
            .and_then(|entries| entries.first_mut())
            .and_then(Value::as_object_mut)
            .expect("API key entry")
            .insert("api-key".to_string(), json!("trusted-source-key-material"));
        set_source_identity(
            &mut trusted_source,
            "trusted-source-instance",
            "trusted-source-credential",
        );
        let mut fixed_section = api_item("codex-api-key", "codex", "openai-responses");
        fixed_section.insert("api-key".to_string(), json!("fixed-section-key-material"));

        let preview = preview_route_credential_import(
            &pool,
            PreviewRouteCredentialImportInput {
                text: Value::Array(vec![
                    Value::Object(compatibility_item("claude")),
                    Value::Object(trusted_source),
                    Value::Object(fixed_section),
                ])
                .to_string(),
                ambiguous_platform_choices: Vec::new(),
            },
        )
        .await
        .expect("preview");

        assert_eq!(preview.items[0].disposition, "possible_duplicate");
        assert_eq!(preview.items[1].disposition, "import");
        assert_eq!(preview.items[2].disposition, "possible_duplicate");
        assert!(preview.items[0].issue_codes.is_empty());
        assert_eq!(preview.counts.importable, 3);
        assert_eq!(preview.counts.duplicates, 0);
        assert_eq!(preview.counts.errors, 0);
    }

    #[tokio::test]
    async fn preview_route_credential_import_groups_batches_counts_and_redacts_output() {
        let pool = migrated_pool().await;

        let mut remote_api = api_item("codex-api-key", "codex", "openai-responses");
        remote_api.insert("type".to_string(), json!("codex-api-key"));
        remote_api.insert("api-key".to_string(), json!("remote-api-secret-marker"));
        metadata_mut(&mut remote_api).insert(
            "display_name".to_string(),
            json!("Remote API Sensitive Name"),
        );
        set_source_identity(
            &mut remote_api,
            "source-instance-sensitive",
            "source-credential-api",
        );
        set_batch(&mut remote_api, Some("remote-batch-sensitive"), "Shared");
        set_in_pool(&mut remote_api);

        let mut remote_official = official_item();
        remote_official.insert(
            "access_token".to_string(),
            json!("remote-official-secret-marker"),
        );
        set_source_identity(
            &mut remote_official,
            "source-instance-sensitive",
            "source-credential-official",
        );
        set_batch(
            &mut remote_official,
            Some("remote-batch-sensitive"),
            "Shared",
        );

        let mut legacy_claude = api_item("claude-api-key", "claude", "anthropic");
        legacy_claude.insert("type".to_string(), json!("claude-api-key"));
        legacy_claude.insert("api-key".to_string(), json!("legacy-claude-secret-marker"));
        set_batch(&mut legacy_claude, Some("legacy-batch-sensitive"), "Shared");
        set_in_pool(&mut legacy_claude);

        let mut legacy_gemini = api_item("gemini-api-key", "gemini", "gemini");
        legacy_gemini.insert("type".to_string(), json!("gemini-api-key"));
        legacy_gemini.insert("api-key".to_string(), json!("legacy-gemini-secret-marker"));
        set_batch(&mut legacy_gemini, Some("legacy-batch-sensitive"), "Shared");

        let mut name_official = official_item();
        name_official.insert(
            "access_token".to_string(),
            json!("name-official-secret-marker"),
        );
        name_official.insert("type".to_string(), json!("claude"));
        metadata_mut(&mut name_official).insert("platform".to_string(), json!("claude"));
        set_batch(&mut name_official, None, "Name only");
        set_in_pool(&mut name_official);

        let mut name_grok = api_item("xai-api-key", "grok", "openai");
        name_grok.insert("type".to_string(), json!("xai-api-key"));
        name_grok.insert("api-key".to_string(), json!("name-grok-secret-marker"));
        name_grok.insert("base-url".to_string(), json!("https://api.x.ai/v1"));
        set_batch(&mut name_grok, None, "Name only");
        set_in_pool(&mut name_grok);

        let choice_required = object(json!({
            "api-key": "choice-required-secret-marker",
            "base-url": endpoint()
        }));
        let input_duplicate = remote_api.clone();
        let source_text = Value::Array(vec![
            Value::Object(remote_api),
            Value::Object(remote_official),
            Value::Object(legacy_claude),
            Value::Object(legacy_gemini),
            Value::Object(name_official),
            Value::Object(name_grok),
            Value::Object(choice_required),
            Value::Object(input_duplicate),
        ])
        .to_string();

        let preview = preview_route_credential_import(
            &pool,
            PreviewRouteCredentialImportInput {
                text: source_text,
                ambiguous_platform_choices: Vec::new(),
            },
        )
        .await
        .expect("preview");

        assert_eq!(preview.counts.total, 8);
        assert_eq!(preview.counts.official, 2);
        assert_eq!(preview.counts.api, 5);
        assert_eq!(preview.counts.importable, 6);
        assert_eq!(preview.counts.duplicates, 1);
        assert_eq!(preview.counts.conflicts, 0);
        assert_eq!(preview.counts.errors, 1);
        assert_eq!(preview.counts.restorable_pool_count, 4);
        assert_eq!(preview.counts.batch_count, 3);
        assert_eq!(
            preview.counts.platform_counts,
            std::collections::BTreeMap::from([
                ("claude".to_string(), 2),
                ("codex".to_string(), 3),
                ("gemini".to_string(), 1),
                ("grok".to_string(), 1),
            ])
        );
        assert_eq!(
            preview.counts.cpa_section_counts,
            std::collections::BTreeMap::from([
                ("claude-api-key".to_string(), 1),
                ("codex-api-key".to_string(), 2),
                ("gemini-api-key".to_string(), 1),
                ("xai-api-key".to_string(), 1),
            ])
        );
        assert_eq!(
            preview.counts.legacy_type_counts,
            preview.counts.cpa_section_counts
        );
        assert_eq!(
            preview.counts.restorable_pool_counts,
            std::collections::BTreeMap::from([
                ("claude".to_string(), 2),
                ("codex".to_string(), 1),
                ("grok".to_string(), 1),
            ])
        );
        assert_eq!(
            preview
                .items
                .iter()
                .map(|item| item.item_index)
                .collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        assert_eq!(preview.items[0].display_name_masked, "R***e");
        assert_eq!(preview.items[6].disposition, "error");
        assert_eq!(
            preview.items[6].issue_codes,
            vec!["transfer.choice_required"]
        );

        let serialized = serde_json::to_string(&preview).expect("serialize preview");
        for sensitive in [
            "remote-api-secret-marker",
            "remote-official-secret-marker",
            "source-instance-sensitive",
            "remote-batch-sensitive",
            "Remote API Sensitive Name",
            "api.example.invalid",
            "fingerprint",
            "secret_payload_json",
            "config_json",
            "preview_json",
        ] {
            assert!(
                !serialized.contains(sensitive),
                "preview leaked {sensitive}"
            );
        }
    }

    #[tokio::test]
    async fn import_route_credentials_reparses_exact_text_and_rechecks_duplicates_in_transaction() {
        let pool = migrated_pool().await;
        let invalid = import_route_credentials(
            &pool,
            ImportRouteCredentialsInput {
                text: "[{".to_string(),
                ambiguous_platform_choices: Vec::new(),
                restore_pool_membership: false,
            },
        )
        .await
        .expect_err("commit must reparse the supplied text");
        assert_eq!(app_error_code(&invalid), "validation.transfer_json_invalid");
        assert_eq!(table_count(&pool, "route_credentials").await, 0);

        let mut source_duplicate = api_item("codex-api-key", "codex", "openai-responses");
        source_duplicate.insert("api-key".to_string(), json!("source-duplicate-key"));
        set_source_identity(&mut source_duplicate, "source-a", "credential-a");
        let source_duplicate_normalized = classify_ok(0, &source_duplicate, &[]);
        insert_origin(
            &pool,
            &source_duplicate_normalized,
            &source_duplicate_normalized.fingerprint,
        )
        .await;

        let mut conflict = api_item("codex-api-key", "codex", "openai-responses");
        conflict.insert("api-key".to_string(), json!("conflict-key"));
        set_source_identity(&mut conflict, "source-b", "credential-b");
        let conflict_normalized = classify_ok(1, &conflict, &[]);
        insert_origin(&pool, &conflict_normalized, "different-fingerprint").await;

        let mut input_duplicate = api_item("codex-api-key", "codex", "openai-responses");
        input_duplicate.insert("api-key".to_string(), json!("input-duplicate-key"));
        let repeated_input = input_duplicate.clone();

        let mut possible_duplicate = api_item("codex-api-key", "codex", "openai-responses");
        possible_duplicate.insert("api-key".to_string(), json!("possible-duplicate-key"));
        let possible_normalized = classify_ok(4, &possible_duplicate, &[]);
        RouteCredentialRepository::create(
            &pool,
            &possible_normalized.platform,
            &possible_normalized.kind,
            "Existing possible duplicate",
            None,
            "ok",
            None,
            &possible_normalized.secret_payload_json,
            &possible_normalized.config_json,
            &possible_normalized.preview_json,
        )
        .await
        .expect("possible duplicate candidate");

        let error = object(json!({
            "api-key": "ambiguous-key",
            "base-url": endpoint()
        }));
        let outcome = import_route_credentials(
            &pool,
            import_input(
                vec![
                    source_duplicate,
                    conflict,
                    input_duplicate,
                    repeated_input,
                    possible_duplicate,
                    error,
                ],
                false,
            ),
        )
        .await
        .expect("transactional import");

        assert_eq!(outcome.imported, 2);
        assert_eq!(outcome.skipped_duplicates, 2);
        assert_eq!(outcome.conflicts, 1);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.restored_pool_members, 0);
        assert_eq!(table_count(&pool, "route_credentials").await, 5);
        assert_eq!(
            table_count(&pool, "route_credential_transfer_origins").await,
            2
        );
    }

    #[tokio::test]
    async fn import_route_credentials_creates_lazy_isolated_batches_and_complete_origins_only() {
        let pool = migrated_pool().await;
        let existing_batch = BatchRepository::create(
            &pool,
            NewBatch {
                name: "Shared".to_string(),
                source: "existing".to_string(),
                notes: None,
            },
        )
        .await
        .expect("existing same-name batch");

        let mut skipped = api_item("codex-api-key", "codex", "openai-responses");
        skipped.insert("api-key".to_string(), json!("skipped-key"));
        set_source_identity(&mut skipped, "source-skipped", "credential-skipped");
        set_batch(&mut skipped, Some("skipped-batch"), "Skipped");
        let skipped_normalized = classify_ok(0, &skipped, &[]);
        insert_origin(&pool, &skipped_normalized, &skipped_normalized.fingerprint).await;

        let mut first = api_item("codex-api-key", "codex", "openai-responses");
        first.insert("api-key".to_string(), json!("first-key"));
        metadata_mut(&mut first).insert("display_name".to_string(), json!("First"));
        set_source_identity(&mut first, "source-one", "credential-one");
        set_batch(&mut first, Some("remote-batch"), "Shared");

        let mut second = first.clone();
        second.insert("api-key".to_string(), json!("second-key"));
        metadata_mut(&mut second).insert("display_name".to_string(), json!("Second"));
        set_source_identity(&mut second, "source-one", "credential-two");

        let mut isolated = first.clone();
        isolated.insert("api-key".to_string(), json!("isolated-key"));
        metadata_mut(&mut isolated).insert("display_name".to_string(), json!("Isolated"));
        set_source_identity(&mut isolated, "source-two", "credential-three");

        let mut name_only = first.clone();
        name_only.insert("api-key".to_string(), json!("name-only-key"));
        metadata_mut(&mut name_only).insert("display_name".to_string(), json!("Name only"));
        metadata_mut(&mut name_only).remove("source_instance_id");
        metadata_mut(&mut name_only).remove("source_credential_id");
        metadata_mut(&mut name_only).remove("source_batch_id");

        let mut no_batch = first.clone();
        no_batch.insert("api-key".to_string(), json!("no-batch-key"));
        metadata_mut(&mut no_batch).insert("display_name".to_string(), json!("No batch"));
        metadata_mut(&mut no_batch).remove("source_credential_id");
        metadata_mut(&mut no_batch).remove("source_batch_id");
        metadata_mut(&mut no_batch).remove("batch_name");

        let outcome = import_route_credentials(
            &pool,
            import_input(
                vec![skipped, first, second, isolated, name_only, no_batch],
                false,
            ),
        )
        .await
        .expect("batch import");

        assert_eq!(outcome.imported, 5);
        assert_eq!(outcome.skipped_duplicates, 1);
        assert_eq!(table_count(&pool, "batches").await, 4);
        assert_eq!(
            table_count(&pool, "route_credential_transfer_origins").await,
            4
        );
        let rows = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT display_name, batch_id FROM route_credentials WHERE display_name != 'Existing origin' ORDER BY display_name",
        )
        .fetch_all(&pool)
        .await
        .expect("imported batch assignments");
        let batch_by_name = rows.into_iter().collect::<HashMap<_, _>>();
        assert_eq!(batch_by_name["First"], batch_by_name["Second"]);
        assert_ne!(batch_by_name["First"], batch_by_name["Isolated"]);
        assert_ne!(batch_by_name["First"], batch_by_name["Name only"]);
        assert_ne!(
            batch_by_name["First"].as_deref(),
            Some(existing_batch.id.as_str())
        );
        assert!(batch_by_name["No batch"].is_none());
        assert_eq!(table_count(&pool, "batches").await, 4);
    }

    #[tokio::test]
    async fn import_route_credentials_restores_pool_membership_only_when_opted_in_and_appends() {
        let pool = migrated_pool().await;
        let existing = RouteCredentialRepository::create(
            &pool,
            "codex",
            "api",
            "Existing pool member",
            None,
            "ok",
            None,
            r#"{"api_key":"existing"}"#,
            r#"{"base_url":"https://existing.invalid"}"#,
            "{}",
        )
        .await
        .expect("existing credential");
        RoutePoolRepository::replace_members(&pool, "codex", &[existing.id.clone()])
            .await
            .expect("existing pool member");

        let mut ignored = api_item("codex-api-key", "codex", "openai-responses");
        ignored.insert("api-key".to_string(), json!("ignored-pool-key"));
        set_in_pool(&mut ignored);
        let ignored_outcome = import_route_credentials(&pool, import_input(vec![ignored], false))
            .await
            .expect("default pool ignore");
        assert_eq!(ignored_outcome.restored_pool_members, 0);
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "codex")
                .await
                .expect("codex members"),
            vec![existing.id.clone()]
        );

        let mut codex = api_item("codex-api-key", "codex", "openai-responses");
        codex.insert("api-key".to_string(), json!("restored-codex-key"));
        set_in_pool(&mut codex);
        let mut gemini = api_item("gemini-api-key", "gemini", "gemini");
        gemini.insert("api-key".to_string(), json!("restored-gemini-key"));
        set_in_pool(&mut gemini);
        let restored = import_route_credentials(&pool, import_input(vec![codex, gemini], true))
            .await
            .expect("pool restore");
        assert_eq!(restored.restored_pool_members, 2);

        let codex_members = RoutePoolRepository::list_member_ids(&pool, "codex")
            .await
            .expect("codex members");
        assert_eq!(codex_members.len(), 2);
        assert_eq!(codex_members[0], existing.id);
        assert_eq!(
            RoutePoolRepository::list_member_ids(&pool, "gemini")
                .await
                .expect("gemini members")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn import_route_credentials_rolls_back_every_write_category_on_sql_failure() {
        let batch_pool = migrated_pool().await;
        sqlx::query(
            "CREATE TRIGGER fail_after_batch BEFORE INSERT ON route_credentials BEGIN SELECT RAISE(FAIL, 'forced credential failure'); END",
        )
        .execute(&batch_pool)
        .await
        .expect("failure trigger");
        let mut batched = api_item("codex-api-key", "codex", "openai-responses");
        set_batch(&mut batched, Some("batch"), "Batch");
        assert!(
            import_route_credentials(&batch_pool, import_input(vec![batched], false))
                .await
                .is_err()
        );
        assert_eq!(table_count(&batch_pool, "batches").await, 0);
        assert_eq!(table_count(&batch_pool, "route_credentials").await, 0);

        let origin_pool = migrated_pool().await;
        sqlx::query(
            "CREATE TRIGGER fail_after_credential BEFORE INSERT ON route_credential_transfer_origins BEGIN SELECT RAISE(FAIL, 'forced origin failure'); END",
        )
        .execute(&origin_pool)
        .await
        .expect("failure trigger");
        let mut complete = api_item("codex-api-key", "codex", "openai-responses");
        set_source_identity(&mut complete, "source", "credential");
        assert!(
            import_route_credentials(&origin_pool, import_input(vec![complete], false))
                .await
                .is_err()
        );
        assert_eq!(table_count(&origin_pool, "route_credentials").await, 0);
        assert_eq!(
            table_count(&origin_pool, "route_credential_transfer_origins").await,
            0
        );

        let later_pool = migrated_pool().await;
        sqlx::query(
            "CREATE TRIGGER fail_after_origin BEFORE INSERT ON route_credentials WHEN NEW.display_name = 'Fail after origin' BEGIN SELECT RAISE(FAIL, 'forced later credential failure'); END",
        )
        .execute(&later_pool)
        .await
        .expect("failure trigger");
        let mut first = api_item("codex-api-key", "codex", "openai-responses");
        first.insert("api-key".to_string(), json!("first-rollback-key"));
        set_source_identity(&mut first, "source", "first");
        let mut later = api_item("codex-api-key", "codex", "openai-responses");
        later.insert("api-key".to_string(), json!("later-rollback-key"));
        metadata_mut(&mut later).insert("display_name".to_string(), json!("Fail after origin"));
        assert!(
            import_route_credentials(&later_pool, import_input(vec![first, later], false))
                .await
                .is_err()
        );
        assert_eq!(table_count(&later_pool, "route_credentials").await, 0);
        assert_eq!(
            table_count(&later_pool, "route_credential_transfer_origins").await,
            0
        );

        let member_pool = migrated_pool().await;
        sqlx::query(
            "CREATE TRIGGER fail_after_member BEFORE INSERT ON route_pool_members WHEN NEW.platform = 'gemini' BEGIN SELECT RAISE(FAIL, 'forced member failure'); END",
        )
        .execute(&member_pool)
        .await
        .expect("failure trigger");
        let mut codex = api_item("codex-api-key", "codex", "openai-responses");
        codex.insert("api-key".to_string(), json!("member-codex-key"));
        set_source_identity(&mut codex, "source", "codex");
        set_batch(&mut codex, Some("pool-batch"), "Pool batch");
        set_in_pool(&mut codex);
        let mut gemini = api_item("gemini-api-key", "gemini", "gemini");
        gemini.insert("api-key".to_string(), json!("member-gemini-key"));
        set_source_identity(&mut gemini, "source", "gemini");
        set_batch(&mut gemini, Some("pool-batch"), "Pool batch");
        set_in_pool(&mut gemini);
        assert!(
            import_route_credentials(&member_pool, import_input(vec![codex, gemini], true))
                .await
                .is_err()
        );
        for table in [
            "batches",
            "route_credentials",
            "route_credential_transfer_origins",
            "route_pool_members",
        ] {
            assert_eq!(table_count(&member_pool, table).await, 0, "{table}");
        }
    }
}
use crate::database::repositories::batch_repository::BatchRepository;
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_credential_transfer_repository::{
    find_origin_by_identity, find_origin_by_identity_tx, get_or_create_installation_id,
    insert_origin_tx, TransferOrigin,
};
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::batch::NewBatch;
use crate::models::platform::{ApiDialect, PlatformId};
use crate::models::route_credential::{normalize_anthropic_api_key_field, ModelMapping};
use crate::models::route_credential_transfer::{
    ImportRouteCredentialsInput, PreviewRouteCredentialImportInput, RouteCredentialImportOutcome,
    RouteCredentialImportPreview, RouteCredentialImportPreviewCounts,
    RouteCredentialImportPreviewItem, RouteCredentialTransferIssue, TransferPlatformChoice,
    TRANSFER_FORMAT, TRANSFER_MAX_BYTES, TRANSFER_MAX_ITEMS, TRANSFER_MAX_ITEM_BYTES,
    TRANSFER_SCHEMA_VERSION,
};
use crate::services::cpa_export_service::{project_credential, trusted_cpa_raw_template};
use crate::services::official_agent_identity_service::{
    is_official_agent_identity_credential, validate_agent_identity_credential_fields,
};
use crate::services::route_credential_transfer_codec::canonical_fingerprint;
use crate::services::route_preview_service::RoutePreviewService;
use chrono::Utc;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        AppError::Database {
            code: "database.sqlx",
            message: "Database operation failed".to_string(),
            details: Some(error.to_string()),
            recoverable: true,
        }
    }
}

const OFFICIAL_SECRET_FIELDS: &[(&str, &[&str])] = &[
    ("id_token", &["id_token", "idToken"]),
    ("access_token", &["access_token", "accessToken"]),
    ("refresh_token", &["refresh_token", "refreshToken"]),
    ("account_id", &["account_id", "accountId"]),
    ("workspace_id", &["workspace_id", "workspaceId"]),
    (
        "chatgpt_account_id",
        &["chatgpt_account_id", "chatgptAccountId"],
    ),
    ("agent_runtime_id", &["agent_runtime_id", "agentRuntimeId"]),
    (
        "agent_private_key",
        &["agent_private_key", "agentPrivateKey"],
    ),
    ("task_id", &["task_id", "taskId"]),
    ("auth_mode", &["auth_mode", "authMode"]),
    (
        "chatgpt_account_is_fedramp",
        &[
            "chatgpt_account_is_fedramp",
            "chatgptAccountIsFedramp",
            "is_fedramp_account",
            "isFedrampAccount",
        ],
    ),
    ("client_id", &["client_id", "clientId"]),
];

const OFFICIAL_CONFIG_FIELDS: &[(&str, &[&str])] = &[
    ("last_refresh", &["last_refresh", "lastRefresh"]),
    ("expired", &["expired"]),
    ("expires_in", &["expires_in", "expiresIn"]),
    ("disabled", &["disabled"]),
    ("base_url", &["base_url", "baseUrl"]),
    ("token_endpoint", &["token_endpoint", "tokenEndpoint"]),
    ("auth_kind", &["auth_kind", "authKind"]),
    ("sub", &["sub"]),
    ("token_type", &["token_type", "tokenType"]),
    ("redirect_uri", &["redirect_uri", "redirectUri"]),
    ("headers", &["headers"]),
];

const TRANSFER_METADATA_FIELDS: &[&str] = &[
    "format",
    "schema_version",
    "source_instance_id",
    "source_credential_id",
    "platform",
    "kind",
    "cpa_section",
    "display_name",
    "source_batch_id",
    "batch_name",
    "in_pool",
    "origin_format",
    "interface_format",
    "responses_custom_tool_compat",
    "api_key_field",
    "model_mappings",
];

pub(crate) struct CompleteSourceIdentity {
    pub(crate) source_instance_id: String,
    pub(crate) source_credential_id: String,
    pub(crate) source_platform: String,
    pub(crate) source_kind: String,
    pub(crate) source_schema_version: i64,
}

pub(crate) struct ImportBatchKey {
    pub(crate) source_instance_id: Option<String>,
    pub(crate) source_batch_id: Option<String>,
    pub(crate) batch_name: String,
}

pub(crate) struct NormalizedImportItem {
    pub(crate) item_index: usize,
    pub(crate) platform: String,
    pub(crate) kind: String,
    pub(crate) cpa_section: Option<String>,
    pub(crate) legacy_type: Option<String>,
    pub(crate) display_name: String,
    pub(crate) display_name_masked: String,
    pub(crate) email: Option<String>,
    pub(crate) secret_payload_json: String,
    pub(crate) config_json: String,
    pub(crate) preview_json: String,
    pub(crate) source_identity: Option<CompleteSourceIdentity>,
    pub(crate) batch_key: Option<ImportBatchKey>,
    pub(crate) in_pool: bool,
    pub(crate) fingerprint: String,
    pub(crate) issue_codes: Vec<String>,
}

struct TransferMetadata {
    schema_version: i64,
    platform: Option<PlatformId>,
    kind: String,
    cpa_section: Option<String>,
    display_name: Option<String>,
    source_instance_id: Option<String>,
    source_credential_id: Option<String>,
    source_batch_id: Option<String>,
    batch_name: Option<String>,
    in_pool: bool,
    interface_format: Option<String>,
    responses_custom_tool_compat: Option<bool>,
    api_key_field: Option<String>,
    model_mappings: Option<Value>,
    issue_codes: Vec<String>,
}

struct ResolvedChoice {
    platform: PlatformId,
    interface_format: String,
    dialect: ApiDialect,
}

struct ApiPayload {
    api_key: String,
    base_url: String,
    headers: Map<String, Value>,
    cpa_models: Vec<Value>,
    issue_codes: Vec<String>,
}

struct ApiTarget {
    platform: PlatformId,
    cpa_section: String,
    interface_format: String,
    dialect: ApiDialect,
    choice_consumed: bool,
}

#[derive(PartialEq, Eq, Hash)]
struct SourceIdentityKey {
    source_instance_id: String,
    source_credential_id: String,
    source_platform: String,
    source_kind: String,
}

#[derive(PartialEq, Eq, Hash)]
enum BatchPredictionKey {
    Source {
        source_instance_id: String,
        source_batch_id: Option<String>,
        batch_name: String,
    },
    Legacy {
        source_batch_id: String,
        batch_name: String,
    },
    Name(String),
}

pub(crate) fn validate_transfer_text(text: &str) -> Result<Vec<Map<String, Value>>, AppError> {
    if text.as_bytes().len() > TRANSFER_MAX_BYTES {
        return Err(validation_error(
            "validation.transfer_text_too_large",
            "Credential transfer JSON exceeds the supported size",
        ));
    }

    let value: Value = serde_json::from_str(text).map_err(|_| {
        validation_error(
            "validation.transfer_json_invalid",
            "Credential transfer JSON is invalid",
        )
    })?;
    let items = value.as_array().ok_or_else(|| {
        validation_error(
            "validation.transfer_array_required",
            "Credential transfer JSON must be a bare array",
        )
    })?;
    if items.len() > TRANSFER_MAX_ITEMS {
        return Err(validation_error(
            "validation.transfer_item_limit",
            "Credential transfer JSON contains too many items",
        ));
    }

    items
        .iter()
        .map(|item| {
            let object = item.as_object().ok_or_else(|| {
                validation_error(
                    "validation.transfer_item_object_required",
                    "Credential transfer entries must be objects",
                )
            })?;
            let compact_size = serde_json::to_vec(object).map_err(|_| {
                validation_error(
                    "validation.transfer_item_invalid",
                    "Credential transfer entry could not be validated",
                )
            })?;
            if compact_size.len() > TRANSFER_MAX_ITEM_BYTES {
                return Err(validation_error(
                    "validation.transfer_item_too_large",
                    "Credential transfer entry exceeds the supported size",
                ));
            }
            Ok(object.clone())
        })
        .collect()
}

pub(crate) fn normalize_transfer_items(
    text: &str,
    choices: &[TransferPlatformChoice],
) -> Result<Vec<Result<NormalizedImportItem, RouteCredentialTransferIssue>>, AppError> {
    let items = validate_transfer_text(text)?;
    validate_choices(&items, choices)?;

    let mut consumed_choices = HashSet::new();
    let normalized = items
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            let (result, choice_consumed) =
                classify_transfer_item_internal(item_index, item, choices);
            if choice_consumed {
                consumed_choices.insert(item_index);
            }
            result
        })
        .collect::<Vec<_>>();

    if choices
        .iter()
        .any(|choice| !consumed_choices.contains(&choice.item_index))
    {
        return Err(validation_error(
            "validation.transfer_choice_unused",
            "A platform choice targets an item that does not require one",
        ));
    }

    Ok(normalized)
}

pub async fn preview_route_credential_import(
    pool: &SqlitePool,
    input: PreviewRouteCredentialImportInput,
) -> Result<RouteCredentialImportPreview, AppError> {
    let normalized = normalize_transfer_items(&input.text, &input.ambiguous_platform_choices)?;
    let platforms = PlatformId::ALL
        .iter()
        .map(|platform| platform.as_str().to_string())
        .collect::<Vec<_>>();
    let instance_id = get_or_create_installation_id(pool).await?;
    let candidates =
        RouteCredentialRepository::list_transfer_fingerprint_candidates(pool, &platforms).await?;
    let mut local_fingerprints = HashSet::new();
    for candidate in candidates {
        let Ok(mut projected) = project_credential(&candidate, &instance_id, false, false) else {
            continue;
        };
        if candidate.kind.trim().eq_ignore_ascii_case("api") {
            if let (Some(section), Some(payload)) = (
                projected.cpa_section.clone(),
                projected.payload.as_object_mut(),
            ) {
                payload.insert("cpa_section".to_string(), Value::String(section));
            }
        }
        let Ok(fingerprint) = canonical_fingerprint(&candidate.kind, &projected.payload) else {
            continue;
        };
        local_fingerprints.insert(fingerprint);
    }

    let mut counts = RouteCredentialImportPreviewCounts {
        total: normalized.len(),
        ..RouteCredentialImportPreviewCounts::default()
    };
    let mut seen_fingerprints = HashSet::new();
    let mut imported_source_identities = HashMap::new();
    let mut batch_groups = HashSet::new();
    let mut items = Vec::with_capacity(normalized.len());

    for result in normalized {
        let item = match result {
            Ok(item) => item,
            Err(issue) => {
                counts.errors += 1;
                items.push(RouteCredentialImportPreviewItem {
                    item_index: issue.item_index.unwrap_or(items.len()),
                    display_name_masked: issue
                        .display_name
                        .unwrap_or_else(|| mask_display_name(&format!("Item {}", items.len() + 1))),
                    platform: None,
                    kind: None,
                    cpa_section: None,
                    disposition: "error".to_string(),
                    issue_codes: vec![issue.code],
                });
                continue;
            }
        };

        increment_count(&mut counts.platform_counts, &item.platform);
        match item.kind.as_str() {
            "official" => counts.official += 1,
            "api" => counts.api += 1,
            _ => {}
        }
        if let Some(cpa_section) = item.cpa_section.as_deref() {
            increment_count(&mut counts.cpa_section_counts, cpa_section);
        }
        if let Some(legacy_type) = item.legacy_type.as_deref() {
            increment_count(&mut counts.legacy_type_counts, legacy_type);
        }

        let disposition = if !seen_fingerprints.insert(item.fingerprint.clone()) {
            counts.duplicates += 1;
            "input_duplicate"
        } else if let Some(identity) = item.source_identity.as_ref() {
            match find_origin_by_identity(
                pool,
                &identity.source_instance_id,
                &identity.source_credential_id,
                &identity.source_platform,
                &identity.source_kind,
            )
            .await?
            {
                Some(origin) if origin.source_fingerprint == item.fingerprint => {
                    counts.duplicates += 1;
                    "source_duplicate"
                }
                Some(_) => {
                    counts.conflicts += 1;
                    "conflict"
                }
                None => {
                    let identity_key = source_identity_key(identity);
                    if imported_source_identities.contains_key(&identity_key) {
                        counts.conflicts += 1;
                        "conflict"
                    } else {
                        imported_source_identities.insert(identity_key, item.fingerprint.clone());
                        counts.importable += 1;
                        "import"
                    }
                }
            }
        } else if local_fingerprints.contains(&item.fingerprint) {
            counts.importable += 1;
            "possible_duplicate"
        } else {
            counts.importable += 1;
            "import"
        };

        let importable = matches!(disposition, "import" | "possible_duplicate");
        if importable {
            if item.in_pool {
                counts.restorable_pool_count += 1;
                increment_count(&mut counts.restorable_pool_counts, &item.platform);
            }
            if let Some(batch_key) = item.batch_key.as_ref().map(batch_prediction_key) {
                batch_groups.insert(batch_key);
            }
        }

        items.push(RouteCredentialImportPreviewItem {
            item_index: item.item_index,
            display_name_masked: item.display_name_masked,
            platform: Some(item.platform),
            kind: Some(item.kind),
            cpa_section: item.cpa_section,
            disposition: disposition.to_string(),
            issue_codes: item.issue_codes,
        });
    }
    counts.batch_count = batch_groups.len();

    Ok(RouteCredentialImportPreview { counts, items })
}

pub async fn import_route_credentials(
    pool: &SqlitePool,
    input: ImportRouteCredentialsInput,
) -> Result<RouteCredentialImportOutcome, AppError> {
    let normalized = normalize_transfer_items(&input.text, &input.ambiguous_platform_choices)?;
    let mut tx = pool.begin().await?;
    let mut outcome = RouteCredentialImportOutcome::default();
    let mut seen_fingerprints = HashSet::new();
    let mut batch_ids = HashMap::<BatchPredictionKey, String>::new();
    let mut pool_members = BTreeMap::<String, Vec<String>>::new();

    for result in normalized {
        let item = match result {
            Ok(item) => item,
            Err(_) => {
                outcome.failed += 1;
                continue;
            }
        };

        if !seen_fingerprints.insert(item.fingerprint.clone()) {
            outcome.skipped_duplicates += 1;
            continue;
        }

        if let Some(identity) = item.source_identity.as_ref() {
            match find_origin_by_identity_tx(
                &mut tx,
                &identity.source_instance_id,
                &identity.source_credential_id,
                &identity.source_platform,
                &identity.source_kind,
            )
            .await?
            {
                Some(origin) if origin.source_fingerprint == item.fingerprint => {
                    outcome.skipped_duplicates += 1;
                    continue;
                }
                Some(_) => {
                    outcome.conflicts += 1;
                    continue;
                }
                None => {}
            }
        }

        let batch_id = if let Some(batch_key) = item.batch_key.as_ref() {
            let prediction_key = batch_prediction_key(batch_key);
            if let Some(batch_id) = batch_ids.get(&prediction_key) {
                Some(batch_id.clone())
            } else {
                let batch = BatchRepository::create_tx(
                    &mut tx,
                    NewBatch {
                        name: batch_key.batch_name.clone(),
                        source: "route_credential_transfer".to_string(),
                        notes: None,
                    },
                )
                .await?;
                batch_ids.insert(prediction_key, batch.id.clone());
                Some(batch.id)
            }
        } else {
            None
        };

        let credential = RouteCredentialRepository::create_tx(
            &mut tx,
            &item.platform,
            &item.kind,
            &item.display_name,
            item.email.clone(),
            "ok",
            batch_id,
            &item.secret_payload_json,
            &item.config_json,
            &item.preview_json,
        )
        .await?;

        if let Some(identity) = item.source_identity.as_ref() {
            insert_origin_tx(
                &mut tx,
                &TransferOrigin {
                    route_credential_id: credential.id.clone(),
                    source_instance_id: identity.source_instance_id.clone(),
                    source_credential_id: identity.source_credential_id.clone(),
                    source_platform: identity.source_platform.clone(),
                    source_kind: identity.source_kind.clone(),
                    source_schema_version: identity.source_schema_version,
                    source_fingerprint: item.fingerprint.clone(),
                    imported_at: Utc::now().to_rfc3339(),
                },
            )
            .await?;
        }

        if input.restore_pool_membership && item.in_pool {
            pool_members
                .entry(item.platform)
                .or_default()
                .push(credential.id);
        }
        outcome.imported += 1;
    }

    for (platform, credential_ids) in pool_members {
        outcome.restored_pool_members +=
            RoutePoolRepository::append_members_tx(&mut tx, &platform, &credential_ids).await?;
    }

    tx.commit().await.map_err(|error| AppError::Database {
        code: "database.route_credential_transfer_import_commit",
        message: "Could not save route credential import".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;
    Ok(outcome)
}

fn increment_count(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

fn source_identity_key(identity: &CompleteSourceIdentity) -> SourceIdentityKey {
    SourceIdentityKey {
        source_instance_id: identity.source_instance_id.clone(),
        source_credential_id: identity.source_credential_id.clone(),
        source_platform: identity.source_platform.clone(),
        source_kind: identity.source_kind.clone(),
    }
}

fn batch_prediction_key(key: &ImportBatchKey) -> BatchPredictionKey {
    if let Some(source_instance_id) = key.source_instance_id.as_ref() {
        BatchPredictionKey::Source {
            source_instance_id: source_instance_id.clone(),
            source_batch_id: key.source_batch_id.clone(),
            batch_name: key.batch_name.clone(),
        }
    } else if let Some(source_batch_id) = key.source_batch_id.as_ref() {
        BatchPredictionKey::Legacy {
            source_batch_id: source_batch_id.clone(),
            batch_name: key.batch_name.clone(),
        }
    } else {
        BatchPredictionKey::Name(key.batch_name.clone())
    }
}

pub(crate) fn classify_transfer_item(
    item_index: usize,
    item: &Map<String, Value>,
    choices: &[TransferPlatformChoice],
) -> Result<NormalizedImportItem, RouteCredentialTransferIssue> {
    classify_transfer_item_internal(item_index, item, choices).0
}

fn classify_transfer_item_internal(
    item_index: usize,
    item: &Map<String, Value>,
    choices: &[TransferPlatformChoice],
) -> (
    Result<NormalizedImportItem, RouteCredentialTransferIssue>,
    bool,
) {
    let display_name = preliminary_display_name(item_index, item);
    let display_name_masked = mask_display_name(&display_name);
    let matching_choices = choices
        .iter()
        .filter(|choice| choice.item_index == item_index)
        .collect::<Vec<_>>();
    if matching_choices.len() > 1 {
        return (
            Err(transfer_issue(
                item_index,
                &display_name_masked,
                "transfer.choice_duplicate",
                Some("item_index"),
            )),
            true,
        );
    }
    let choice = matching_choices.first().copied();

    if item.contains_key("api-keys") {
        return (
            Err(transfer_issue(
                item_index,
                &display_name_masked,
                "transfer.top_level_api_keys_unsupported",
                Some("api-keys"),
            )),
            false,
        );
    }

    let metadata = match parse_transfer_metadata(item_index, item, &display_name_masked) {
        Ok(metadata) => metadata,
        Err(issue) => return (Err(issue), false),
    };
    let legacy_type = string_value(item, &["type"]).map(|value| normalize_cpa_section(&value));
    if legacy_type
        .as_deref()
        .is_some_and(is_explicitly_unsupported_section)
    {
        return (
            Err(transfer_issue(
                item_index,
                &display_name_masked,
                "transfer.cpa_section_unsupported",
                Some("type"),
            )),
            false,
        );
    }

    if let Some(metadata) = metadata.as_ref() {
        if metadata
            .cpa_section
            .as_deref()
            .is_some_and(is_explicitly_unsupported_section)
        {
            return (
                Err(transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.cpa_section_unsupported",
                    Some("cpa_section"),
                )),
                false,
            );
        }

        return match metadata.kind.as_str() {
            "official" => (
                normalize_official_item(item_index, item, metadata, legacy_type.as_deref()),
                false,
            ),
            "api" => {
                let (result, consumed) =
                    normalize_api_item(item_index, item, metadata, legacy_type.as_deref(), choice);
                (result, consumed)
            }
            _ => unreachable!("metadata kind is validated"),
        };
    }

    if let Some(raw_type) = string_value(item, &["type"]) {
        if let Ok(platform) = PlatformId::parse(&raw_type) {
            if has_api_shape(item) {
                return (
                    Err(transfer_issue(
                        item_index,
                        &display_name_masked,
                        "transfer.metadata_conflict",
                        Some("type"),
                    )),
                    false,
                );
            }
            let metadata = metadata_for_legacy_official(platform);
            return (
                normalize_official_item(item_index, item, &metadata, None),
                false,
            );
        }
    }

    if legacy_type.as_deref().is_some_and(is_supported_api_section) || has_api_shape(item) {
        let metadata = metadata_for_legacy_api(legacy_type.as_deref());
        let (result, consumed) =
            normalize_api_item(item_index, item, &metadata, legacy_type.as_deref(), choice);
        return (result, consumed);
    }

    (
        Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.credential_kind_unrecognized",
            Some("type"),
        )),
        false,
    )
}

fn validate_choices(
    items: &[Map<String, Value>],
    choices: &[TransferPlatformChoice],
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for choice in choices {
        if !seen.insert(choice.item_index) {
            return Err(validation_error(
                "validation.transfer_choice_duplicate",
                "Each transfer item may have at most one platform choice",
            ));
        }
        if choice.item_index >= items.len() {
            return Err(validation_error(
                "validation.transfer_choice_target_missing",
                "A platform choice targets a missing transfer item",
            ));
        }
        PlatformId::parse(&choice.platform).map_err(|_| {
            validation_error(
                "validation.transfer_choice_platform",
                "A transfer platform choice is not supported",
            )
        })?;
        if let Some(interface_format) = choice.interface_format.as_deref() {
            ApiDialect::parse(interface_format).map_err(|_| {
                validation_error(
                    "validation.transfer_choice_interface_format",
                    "A transfer interface choice is not supported",
                )
            })?;
        }
    }
    Ok(())
}

fn parse_transfer_metadata(
    item_index: usize,
    item: &Map<String, Value>,
    display_name_masked: &str,
) -> Result<Option<TransferMetadata>, RouteCredentialTransferIssue> {
    let Some(value) = item.get("x-ai-switch") else {
        return Ok(None);
    };
    let object = value.as_object().ok_or_else(|| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.metadata_invalid",
            Some("x-ai-switch"),
        )
    })?;
    for (field, value) in object {
        if TRANSFER_METADATA_FIELDS.contains(&field.as_str())
            && nested_value_contains_secret_field(value)
        {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_secret_field_unsupported",
                Some("unknown_secret_field"),
            ));
        }
    }
    if object.get("format").and_then(Value::as_str) != Some(TRANSFER_FORMAT) {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.metadata_format_unsupported",
            Some("format"),
        ));
    }
    let schema_version = object
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_invalid",
                Some("schema_version"),
            )
        })?;
    if schema_version != i64::from(TRANSFER_SCHEMA_VERSION) {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.schema_version_unsupported",
            Some("schema_version"),
        ));
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|kind| matches!(kind.as_str(), "official" | "api"))
        .ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_invalid",
                Some("kind"),
            )
        })?;
    let platform = match object.get("platform") {
        Some(Value::String(platform)) if !platform.trim().is_empty() => {
            Some(PlatformId::parse(platform).map_err(|_| {
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.metadata_invalid",
                    Some("platform"),
                )
            })?)
        }
        Some(Value::String(_)) | Some(Value::Null) | None => None,
        Some(_) => {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_invalid",
                Some("platform"),
            ));
        }
    };
    let cpa_section = match object.get("cpa_section") {
        Some(Value::String(value)) if !value.trim().is_empty() => {
            Some(normalize_cpa_section(value))
        }
        Some(Value::String(_)) | Some(Value::Null) | None => None,
        Some(_) => {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_invalid",
                Some("cpa_section"),
            ));
        }
    };
    if kind == "official" && cpa_section.is_some() {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.metadata_conflict",
            Some("cpa_section"),
        ));
    }
    if kind == "api" && cpa_section.is_none() {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.metadata_invalid",
            Some("cpa_section"),
        ));
    }

    let mut issue_codes = Vec::new();
    for (field, value) in object {
        if !is_nonempty(value) {
            continue;
        }
        if TRANSFER_METADATA_FIELDS.contains(&field.as_str()) {
            if matches!(
                field.as_str(),
                "display_name"
                    | "source_instance_id"
                    | "source_credential_id"
                    | "source_batch_id"
                    | "batch_name"
                    | "origin_format"
                    | "interface_format"
                    | "api_key_field"
            ) && !matches!(value, Value::String(_) | Value::Null)
            {
                push_issue_code(&mut issue_codes, "transfer.metadata_field_ignored");
            }
            continue;
        }
        if field_is_secret_bearing(field, value) {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_secret_field_unsupported",
                Some("unknown_secret_field"),
            ));
        }
        push_issue_code(&mut issue_codes, "transfer.metadata_field_ignored");
    }
    let in_pool = match object.get("in_pool") {
        Some(Value::Bool(value)) => *value,
        Some(Value::Null) | None => false,
        Some(_) => {
            push_issue_code(&mut issue_codes, "transfer.metadata_field_ignored");
            false
        }
    };
    let responses_custom_tool_compat = match object.get("responses_custom_tool_compat") {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Null) | None => None,
        Some(_) => {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.metadata_invalid",
                Some("responses_custom_tool_compat"),
            ));
        }
    };
    let model_mappings = validate_metadata_model_mappings(
        item_index,
        display_name_masked,
        object.get("model_mappings"),
        &mut issue_codes,
    )?;

    Ok(Some(TransferMetadata {
        schema_version,
        platform,
        kind,
        cpa_section,
        display_name: optional_string(object, "display_name"),
        source_instance_id: optional_string(object, "source_instance_id"),
        source_credential_id: optional_string(object, "source_credential_id"),
        source_batch_id: optional_string(object, "source_batch_id"),
        batch_name: optional_string(object, "batch_name"),
        in_pool,
        interface_format: optional_string(object, "interface_format"),
        responses_custom_tool_compat,
        api_key_field: optional_string(object, "api_key_field"),
        model_mappings,
        issue_codes,
    }))
}

fn validate_metadata_model_mappings(
    item_index: usize,
    display_name_masked: &str,
    value: Option<&Value>,
    issue_codes: &mut Vec<String>,
) -> Result<Option<Value>, RouteCredentialTransferIssue> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(entries) = value.as_array() else {
        if value.is_null() {
            return Ok(None);
        }
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        ));
    };

    let mut normalized = Vec::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object().ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.model_mappings_conflict",
                Some("model_mappings"),
            )
        })?;
        for (field, field_value) in object {
            if ["from", "to", "label", "supports_1m"].contains(&field.as_str())
                || !is_nonempty(field_value)
            {
                continue;
            }
            if field_is_secret_bearing(field, field_value) {
                return Err(transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.metadata_secret_field_unsupported",
                    Some("unknown_secret_field"),
                ));
            }
            push_issue_code(issue_codes, "transfer.metadata_field_ignored");
        }
        let typed = serde_json::from_value::<ModelMapping>(Value::Object(object.clone())).map_err(
            |_| {
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.model_mappings_conflict",
                    Some("model_mappings"),
                )
            },
        )?;
        if typed.from.trim().is_empty() || typed.to.trim().is_empty() {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.model_mappings_conflict",
                Some("model_mappings"),
            ));
        }
        let mut normalized_entry = Map::from_iter([
            ("from".to_string(), json!(typed.from.trim())),
            ("to".to_string(), json!(typed.to.trim())),
        ]);
        if let Some(label) = typed
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
        {
            normalized_entry.insert("label".to_string(), json!(label));
        }
        if let Some(supports_1m) = typed.supports_1m {
            normalized_entry.insert("supports_1m".to_string(), json!(supports_1m));
        }
        normalized.push(Value::Object(normalized_entry));
    }
    Ok(Some(Value::Array(normalized)))
}

fn metadata_for_legacy_official(platform: PlatformId) -> TransferMetadata {
    TransferMetadata {
        schema_version: i64::from(TRANSFER_SCHEMA_VERSION),
        platform: Some(platform),
        kind: "official".to_string(),
        cpa_section: None,
        display_name: None,
        source_instance_id: None,
        source_credential_id: None,
        source_batch_id: None,
        batch_name: None,
        in_pool: false,
        interface_format: None,
        responses_custom_tool_compat: None,
        api_key_field: None,
        model_mappings: None,
        issue_codes: Vec::new(),
    }
}

fn metadata_for_legacy_api(legacy_type: Option<&str>) -> TransferMetadata {
    TransferMetadata {
        schema_version: i64::from(TRANSFER_SCHEMA_VERSION),
        platform: legacy_type.and_then(fixed_platform_for_section),
        kind: "api".to_string(),
        cpa_section: legacy_type
            .filter(|value| is_supported_api_section(value))
            .map(str::to_string),
        display_name: None,
        source_instance_id: None,
        source_credential_id: None,
        source_batch_id: None,
        batch_name: None,
        in_pool: false,
        interface_format: None,
        responses_custom_tool_compat: None,
        api_key_field: None,
        model_mappings: None,
        issue_codes: Vec::new(),
    }
}

fn normalize_official_item(
    item_index: usize,
    item: &Map<String, Value>,
    metadata: &TransferMetadata,
    legacy_type: Option<&str>,
) -> Result<NormalizedImportItem, RouteCredentialTransferIssue> {
    let display_name = final_display_name(item_index, item, metadata.display_name.as_deref());
    let display_name_masked = mask_display_name(&display_name);
    if has_api_shape(item) || legacy_type.is_some_and(is_supported_api_section) {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.metadata_conflict",
            Some("kind"),
        ));
    }

    let raw_type = string_value(item, &["type"]).ok_or_else(|| {
        transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.official_type_required",
            Some("type"),
        )
    })?;
    let raw_platform = PlatformId::parse(&raw_type).map_err(|_| {
        transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.official_type_unsupported",
            Some("type"),
        )
    })?;
    let platform = metadata.platform.unwrap_or(raw_platform);
    if platform != raw_platform {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.metadata_conflict",
            Some("platform"),
        ));
    }
    if !is_supported_official_platform(platform) {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.official_platform_unsupported",
            Some("platform"),
        ));
    }

    let mut issue_codes = metadata.issue_codes.clone();
    let mut normalized_raw = item.clone();
    normalized_raw.remove("x-ai-switch");
    for field in [
        "raw",
        "raw_type",
        "import_format",
        "preview",
        "preview_json",
        "id",
        "credential_id",
        "batch_id",
        "batch_name",
        "status",
        "sort_order",
        "subscription_type",
        "primary_remain",
        "weekly_remain",
        "reset_primary",
        "reset_weekly",
        "transient_failure_count",
        "next_retry_at",
        "cooldown_until",
        "last_failure_kind",
        "last_failure_message",
        "request_count",
        "success_count",
        "failure_count",
        "success_rate",
        "quota_remaining",
        "quota_limit",
        "quota_used",
        "quota_updated_at",
        "created_at",
        "updated_at",
    ] {
        normalized_raw.remove(field);
    }
    if normalized_raw.contains_key("credentials") || normalized_raw.contains_key("tokens") {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.official_nested_credentials_unsupported",
            Some("credentials"),
        ));
    }

    let mut secret = Map::new();
    for (canonical, aliases) in OFFICIAL_SECRET_FIELDS {
        if let Some(value) = first_value(item, aliases).filter(|value| is_nonempty(value)) {
            secret.insert((*canonical).to_string(), value.clone());
        }
        normalize_raw_aliases(&mut normalized_raw, canonical, aliases, item);
    }
    let email = string_value(item, &["email"]);
    normalize_raw_aliases(&mut normalized_raw, "email", &["email"], item);

    let mut config = Map::new();
    config.insert("type".to_string(), json!(platform.as_str()));
    for (canonical, aliases) in OFFICIAL_CONFIG_FIELDS {
        if let Some(value) = first_value(item, aliases).filter(|value| is_nonempty(value)) {
            if *canonical == "headers" && !value.is_object() {
                return Err(transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.official_field_invalid",
                    Some("headers"),
                ));
            }
            config.insert((*canonical).to_string(), value.clone());
        }
        normalize_raw_aliases(&mut normalized_raw, canonical, aliases, item);
    }

    let cpa_type = official_cpa_type(platform);
    normalized_raw.insert("type".to_string(), json!(cpa_type));
    config.insert("raw_type".to_string(), json!(cpa_type));
    config.insert("import_format".to_string(), json!("auth-file"));
    config.insert("raw".to_string(), Value::Object(normalized_raw.clone()));
    if !trusted_cpa_raw_template(platform.as_str(), &Value::Object(config.clone())) {
        push_issue_code(&mut issue_codes, "transfer.untrusted_raw_discarded");
        normalized_raw = official_allowlisted_raw(cpa_type, email.as_deref(), &secret, &config);
        config.insert("raw".to_string(), Value::Object(normalized_raw.clone()));
    }
    if !trusted_cpa_raw_template(platform.as_str(), &Value::Object(config.clone())) {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.official_raw_invalid",
            Some("raw"),
        ));
    }

    let secret_value = Value::Object(secret.clone());
    let config_value = Value::Object(config.clone());
    if is_official_agent_identity_credential(&secret_value, &config_value) {
        validate_agent_identity_credential_fields(&secret_value, &config_value).map_err(
            |field| {
                transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.agent_identity_field_required",
                    Some(field),
                )
            },
        )?;
    } else if !has_nonempty_field(&secret, "access_token")
        && !has_nonempty_field(&secret, "refresh_token")
    {
        return Err(transfer_issue(
            item_index,
            &display_name_masked,
            "transfer.oauth_token_required",
            Some("access_token"),
        ));
    }

    let fingerprint =
        canonical_fingerprint("official", &Value::Object(normalized_raw)).map_err(|_| {
            transfer_issue(
                item_index,
                &display_name_masked,
                "transfer.fingerprint_invalid",
                None,
            )
        })?;
    let secret_payload_json = Value::Object(secret).to_string();
    let config_json = Value::Object(config).to_string();
    let preview_json = RoutePreviewService::generate(
        platform.as_str(),
        "official",
        &secret_payload_json,
        &config_json,
    );
    let (source_identity, batch_key) =
        source_context(metadata, platform, "official", &mut issue_codes);

    Ok(NormalizedImportItem {
        item_index,
        platform: platform.as_str().to_string(),
        kind: "official".to_string(),
        cpa_section: None,
        legacy_type: None,
        display_name,
        display_name_masked,
        email,
        secret_payload_json,
        config_json,
        preview_json,
        source_identity,
        batch_key,
        in_pool: metadata.in_pool,
        fingerprint,
        issue_codes,
    })
}

fn normalize_api_item(
    item_index: usize,
    item: &Map<String, Value>,
    metadata: &TransferMetadata,
    legacy_type: Option<&str>,
    choice: Option<&TransferPlatformChoice>,
) -> (
    Result<NormalizedImportItem, RouteCredentialTransferIssue>,
    bool,
) {
    let display_name = final_display_name(item_index, item, metadata.display_name.as_deref());
    let display_name_masked = mask_display_name(&display_name);
    if has_official_auth_shape(item)
        || string_value(item, &["type"])
            .and_then(|value| PlatformId::parse(&value).ok())
            .is_some()
    {
        return (
            Err(transfer_issue(
                item_index,
                &display_name_masked,
                "transfer.metadata_conflict",
                Some("kind"),
            )),
            false,
        );
    }
    if let (Some(metadata_section), Some(legacy_section)) =
        (metadata.cpa_section.as_deref(), legacy_type)
    {
        if is_supported_api_section(legacy_section) && metadata_section != legacy_section {
            return (
                Err(transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.metadata_conflict",
                    Some("cpa_section"),
                )),
                false,
            );
        }
    }

    let section_hint = metadata
        .cpa_section
        .as_deref()
        .or_else(|| legacy_type.filter(|value| is_supported_api_section(value)));
    let choice_required = api_choice_required(metadata, section_hint);
    let payload = match parse_api_payload(item_index, item, section_hint, &display_name_masked) {
        Ok(payload) => payload,
        Err(issue) => return (Err(issue), choice_required && choice.is_some()),
    };
    let target = match resolve_api_target(
        item_index,
        &display_name_masked,
        metadata,
        section_hint,
        &payload.base_url,
        choice,
    ) {
        Ok(target) => target,
        Err((issue, consumed)) => return (Err(issue), consumed),
    };

    let mut issue_codes = metadata.issue_codes.clone();
    for code in payload.issue_codes {
        push_issue_code(&mut issue_codes, &code);
    }
    let reversed_mappings =
        match reverse_cpa_models(item_index, &display_name_masked, &payload.cpa_models) {
            Ok(mappings) => mappings,
            Err(issue) => return (Err(issue), target.choice_consumed),
        };
    let model_mappings = match restore_model_mappings(
        item_index,
        &display_name_masked,
        &reversed_mappings,
        metadata.model_mappings.as_ref(),
    ) {
        Ok(mappings) => mappings,
        Err(issue) => return (Err(issue), target.choice_consumed),
    };

    let mut config = Map::from_iter([
        ("base_url".to_string(), json!(payload.base_url)),
        (
            "interface_format".to_string(),
            json!(target.interface_format),
        ),
        ("model_mappings".to_string(), model_mappings.clone()),
        (
            "responses_custom_tool_compat".to_string(),
            json!(metadata.responses_custom_tool_compat.unwrap_or(false)),
        ),
    ]);
    if !payload.headers.is_empty() {
        config.insert(
            "headers".to_string(),
            Value::Object(payload.headers.clone()),
        );
    }
    if let Some(api_key_field) = metadata.api_key_field.as_deref() {
        if target.dialect != ApiDialect::Anthropic {
            return (
                Err(transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.api_key_field_conflict",
                    Some("api_key_field"),
                )),
                target.choice_consumed,
            );
        }
        let api_key_field = match normalize_anthropic_api_key_field(Some(api_key_field)) {
            Ok(value) => value,
            Err(_) => {
                return (
                    Err(transfer_issue(
                        item_index,
                        &display_name_masked,
                        "transfer.api_key_field_conflict",
                        Some("api_key_field"),
                    )),
                    target.choice_consumed,
                );
            }
        };
        config.insert("api_key_field".to_string(), json!(api_key_field));
    }

    let secret_payload_json = json!({ "api_key": payload.api_key }).to_string();
    let config_json = Value::Object(config.clone()).to_string();
    let preview_json = RoutePreviewService::generate(
        target.platform.as_str(),
        "api",
        &secret_payload_json,
        &config_json,
    );
    let fingerprint_input = json!({
        "credential": api_fingerprint_credential(item, &target.cpa_section),
        "cpa_section": target.cpa_section,
        "interface_format": target.interface_format,
        "model_mappings": model_mappings,
        "responses_custom_tool_compat": metadata.responses_custom_tool_compat.unwrap_or(false),
        "api_key_field": config.get("api_key_field").and_then(Value::as_str).unwrap_or_default(),
    });
    let fingerprint = match canonical_fingerprint("api", &fingerprint_input) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return (
                Err(transfer_issue(
                    item_index,
                    &display_name_masked,
                    "transfer.fingerprint_invalid",
                    None,
                )),
                target.choice_consumed,
            );
        }
    };
    let (source_identity, batch_key) =
        source_context(metadata, target.platform, "api", &mut issue_codes);

    (
        Ok(NormalizedImportItem {
            item_index,
            platform: target.platform.as_str().to_string(),
            kind: "api".to_string(),
            cpa_section: Some(target.cpa_section),
            legacy_type: legacy_type
                .filter(|value| is_supported_api_section(value))
                .map(str::to_string),
            display_name,
            display_name_masked,
            email: None,
            secret_payload_json,
            config_json,
            preview_json,
            source_identity,
            batch_key,
            in_pool: metadata.in_pool,
            fingerprint,
            issue_codes,
        }),
        target.choice_consumed,
    )
}

fn parse_api_payload(
    item_index: usize,
    item: &Map<String, Value>,
    section_hint: Option<&str>,
    display_name_masked: &str,
) -> Result<ApiPayload, RouteCredentialTransferIssue> {
    let compatibility = section_hint == Some("openai-compatibility")
        || (section_hint.is_none()
            && (item.contains_key("api-key-entries") || item.contains_key("api_key_entries")));
    let mut issue_codes = Vec::new();
    let api_key = if compatibility {
        let entries = item
            .get("api-key-entries")
            .or_else(|| item.get("api_key_entries"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.api_key_entries_count",
                    Some("api-key-entries"),
                )
            })?;
        if entries.len() != 1 {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.api_key_entries_count",
                Some("api-key-entries"),
            ));
        }
        let entry = entries[0].as_object().ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.api_key_entry_invalid",
                Some("api-key-entries"),
            )
        })?;
        for (field, value) in entry {
            if field == "api-key" || !is_nonempty(value) {
                continue;
            }
            if field_is_secret_bearing(field, value) {
                return Err(transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.api_secret_field_unsupported",
                    Some("unknown_secret_field"),
                ));
            }
            push_issue_code(&mut issue_codes, "transfer.api_field_ignored");
        }
        string_value(entry, &["api-key", "api_key"]).ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.api_key_required",
                Some("api-key"),
            )
        })?
    } else {
        string_value(item, &["api-key", "api_key"]).ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.api_key_required",
                Some("api-key"),
            )
        })?
    };
    let base_url = string_value(item, &["base-url", "base_url"]).ok_or_else(|| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.base_url_required",
            Some("base-url"),
        )
    })?;
    validate_api_url(&base_url)
        .map_err(|code| transfer_issue(item_index, display_name_masked, code, Some("base-url")))?;
    let headers = match item.get("headers") {
        Some(Value::Object(headers)) => headers.clone(),
        Some(Value::Null) | None => Map::new(),
        Some(_) => {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.headers_invalid",
                Some("headers"),
            ));
        }
    };
    let cpa_models = match item.get("models") {
        Some(Value::Array(models)) => {
            validate_cpa_model_entries(item_index, display_name_masked, models, &mut issue_codes)?;
            models.clone()
        }
        Some(Value::Null) | None => Vec::new(),
        Some(_) => {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.models_invalid",
                Some("models"),
            ));
        }
    };

    let allowed_fields = if compatibility {
        &[
            "type",
            "name",
            "base-url",
            "headers",
            "api-key-entries",
            "api_key_entries",
            "models",
            "x-ai-switch",
        ][..]
    } else {
        &[
            "type",
            "api-key",
            "api_key",
            "base-url",
            "base_url",
            "headers",
            "models",
            "name",
            "x-ai-switch",
        ][..]
    };
    for (field, value) in item {
        if allowed_fields.contains(&field.as_str()) || !is_nonempty(value) {
            continue;
        }
        if field_is_secret_bearing(field, value) {
            return Err(transfer_issue(
                item_index,
                display_name_masked,
                "transfer.api_secret_field_unsupported",
                Some("unknown_secret_field"),
            ));
        }
        push_issue_code(&mut issue_codes, "transfer.api_field_ignored");
    }

    Ok(ApiPayload {
        api_key,
        base_url,
        headers,
        cpa_models,
        issue_codes,
    })
}

fn validate_cpa_model_entries(
    item_index: usize,
    display_name_masked: &str,
    models: &[Value],
    issue_codes: &mut Vec<String>,
) -> Result<(), RouteCredentialTransferIssue> {
    for model in models {
        let object = model.as_object().ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.models_invalid",
                Some("models"),
            )
        })?;
        for (field, value) in object {
            if ["name", "alias", "display-name", "max-context-length"].contains(&field.as_str()) {
                let valid = if field == "max-context-length" {
                    value.is_null() || value.as_u64().is_some()
                } else {
                    value.is_null() || value.as_str().is_some()
                };
                if !valid {
                    return Err(transfer_issue(
                        item_index,
                        display_name_masked,
                        "transfer.models_invalid",
                        Some("models"),
                    ));
                }
                continue;
            }
            if !is_nonempty(value) {
                continue;
            }
            if field_is_secret_bearing(field, value) {
                return Err(transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.api_secret_field_unsupported",
                    Some("unknown_secret_field"),
                ));
            }
            push_issue_code(issue_codes, "transfer.api_field_ignored");
        }
    }
    Ok(())
}

fn resolve_api_target(
    item_index: usize,
    display_name_masked: &str,
    metadata: &TransferMetadata,
    section_hint: Option<&str>,
    base_url: &str,
    choice: Option<&TransferPlatformChoice>,
) -> Result<ApiTarget, (RouteCredentialTransferIssue, bool)> {
    let mut choice_consumed = false;
    let resolved_choice = choice
        .map(|choice| resolve_choice(item_index, display_name_masked, choice))
        .transpose()
        .map_err(|issue| (issue, true))?;

    let mut cpa_section = section_hint.map(str::to_string);
    let expected_dialect = cpa_section
        .as_deref()
        .and_then(expected_dialect_for_section);
    if cpa_section.is_some() && expected_dialect.is_none() {
        return Err((
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.cpa_section_unsupported",
                Some("cpa_section"),
            ),
            false,
        ));
    }

    let metadata_interface = metadata
        .interface_format
        .as_deref()
        .map(|value| ApiDialect::parse(value).map(|dialect| (value.trim().to_string(), dialect)));
    let metadata_interface = match metadata_interface {
        Some(Ok(value)) => Some(value),
        Some(Err(_)) => {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.interface_format_conflict",
                    Some("interface_format"),
                ),
                false,
            ));
        }
        None => None,
    };
    if let (Some((_, dialect)), Some(expected)) = (metadata_interface.as_ref(), expected_dialect) {
        if *dialect != expected {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.interface_format_conflict",
                    Some("interface_format"),
                ),
                false,
            ));
        }
    }

    let requires_choice = cpa_section.is_none()
        || (cpa_section.as_deref() == Some("openai-compatibility") && metadata.platform.is_none());
    if requires_choice {
        choice_consumed = true;
    }
    let resolved_choice = if requires_choice {
        Some(resolved_choice.as_ref().ok_or_else(|| {
            (
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.choice_required",
                    Some("item_index"),
                ),
                true,
            )
        })?)
    } else {
        resolved_choice.as_ref()
    };

    let platform = metadata
        .platform
        .or_else(|| cpa_section.as_deref().and_then(fixed_platform_for_section))
        .or_else(|| resolved_choice.map(|choice| choice.platform))
        .ok_or_else(|| {
            (
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.choice_required",
                    Some("platform"),
                ),
                true,
            )
        })?;
    let (interface_format, dialect) = if let Some(value) = metadata_interface {
        value
    } else if let Some(choice) = resolved_choice {
        (choice.interface_format.clone(), choice.dialect)
    } else if let Some(expected) = expected_dialect {
        (expected.as_str().to_string(), expected)
    } else {
        return Err((
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.choice_required",
                Some("interface_format"),
            ),
            true,
        ));
    };

    if let Some(choice) = resolved_choice {
        if choice.platform != platform || choice.dialect != dialect {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.choice_incompatible",
                    Some("interface_format"),
                ),
                true,
            ));
        }
    }

    if let Some(section) = cpa_section.as_deref() {
        if let Some(expected_platform) = fixed_platform_for_section(section) {
            if platform != expected_platform {
                return Err((
                    transfer_issue(
                        item_index,
                        display_name_masked,
                        "transfer.metadata_conflict",
                        Some("platform"),
                    ),
                    choice_consumed,
                ));
            }
        }
        if expected_dialect_for_section(section) != Some(dialect) {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.interface_format_conflict",
                    Some("interface_format"),
                ),
                choice_consumed,
            ));
        }
    }

    let derived_section = derive_api_section(platform, dialect, base_url);
    if let Some(expected_platform) = fixed_platform_for_section(derived_section) {
        if platform != expected_platform {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    if choice_consumed {
                        "transfer.choice_incompatible"
                    } else {
                        "transfer.metadata_conflict"
                    },
                    Some("platform"),
                ),
                choice_consumed,
            ));
        }
    }
    if let Some(section) = cpa_section.as_deref() {
        if section != derived_section {
            return Err((
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.interface_format_conflict",
                    Some("cpa_section"),
                ),
                choice_consumed,
            ));
        }
    } else {
        cpa_section = Some(derived_section.to_string());
    }

    Ok(ApiTarget {
        platform,
        cpa_section: cpa_section.expect("derived CPA section"),
        interface_format,
        dialect,
        choice_consumed,
    })
}

fn api_choice_required(metadata: &TransferMetadata, section_hint: Option<&str>) -> bool {
    section_hint.is_none()
        || (section_hint == Some("openai-compatibility") && metadata.platform.is_none())
}

fn resolve_choice(
    item_index: usize,
    display_name_masked: &str,
    choice: &TransferPlatformChoice,
) -> Result<ResolvedChoice, RouteCredentialTransferIssue> {
    let platform = PlatformId::parse(&choice.platform).map_err(|_| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.choice_incompatible",
            Some("platform"),
        )
    })?;
    let interface_format = choice
        .interface_format
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.choice_incompatible",
                Some("interface_format"),
            )
        })?;
    let dialect = ApiDialect::parse(interface_format).map_err(|_| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.choice_incompatible",
            Some("interface_format"),
        )
    })?;
    Ok(ResolvedChoice {
        platform,
        interface_format: interface_format.to_string(),
        dialect,
    })
}

fn reverse_cpa_models(
    item_index: usize,
    display_name_masked: &str,
    models: &[Value],
) -> Result<Vec<Value>, RouteCredentialTransferIssue> {
    models
        .iter()
        .map(|model| {
            let model = model.as_object().ok_or_else(|| {
                transfer_issue(
                    item_index,
                    display_name_masked,
                    "transfer.models_invalid",
                    Some("models"),
                )
            })?;
            let mut mapping = Map::new();
            for (source, target) in [("name", "to"), ("alias", "from"), ("display-name", "label")] {
                if let Some(value) = model
                    .get(source)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    mapping.insert(target.to_string(), json!(value));
                }
            }
            if model
                .get("max-context-length")
                .and_then(Value::as_u64)
                .is_some_and(|value| value >= 1_048_576)
            {
                mapping.insert("supports_1m".to_string(), json!(true));
            }
            Ok(Value::Object(mapping))
        })
        .collect()
}

fn restore_model_mappings(
    item_index: usize,
    display_name_masked: &str,
    reversed_mappings: &[Value],
    metadata_mappings: Option<&Value>,
) -> Result<Value, RouteCredentialTransferIssue> {
    let reversed = Value::Array(reversed_mappings.to_vec());
    let Some(metadata_mappings) = metadata_mappings else {
        return Ok(reversed);
    };
    if !metadata_mappings.is_array() {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        ));
    }
    let typed_metadata = serde_json::from_value::<Vec<ModelMapping>>(metadata_mappings.clone())
        .map_err(|_| {
            transfer_issue(
                item_index,
                display_name_masked,
                "transfer.model_mappings_conflict",
                Some("model_mappings"),
            )
        })?;
    if typed_metadata
        .iter()
        .any(|mapping| mapping.from.trim().is_empty() || mapping.to.trim().is_empty())
    {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        ));
    }
    let normalized_metadata = serde_json::to_value(&typed_metadata).map_err(|_| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        )
    })?;
    let metadata_signature = model_mapping_signature(
        normalized_metadata
            .as_array()
            .expect("typed model mappings serialize as an array"),
    )
    .ok_or_else(|| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        )
    })?;
    let reversed_signature = model_mapping_signature(reversed_mappings).ok_or_else(|| {
        transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("models"),
        )
    })?;
    if metadata_signature != reversed_signature {
        return Err(transfer_issue(
            item_index,
            display_name_masked,
            "transfer.model_mappings_conflict",
            Some("model_mappings"),
        ));
    }
    Ok(normalized_metadata)
}

fn model_mapping_signature(mappings: &[Value]) -> Option<Vec<String>> {
    let mut signature = mappings
        .iter()
        .map(|mapping| {
            let mapping = mapping.as_object()?;
            let mut normalized = Map::new();
            for field in ["from", "to", "label"] {
                if let Some(value) = mapping
                    .get(field)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    normalized.insert(field.to_string(), json!(value));
                }
            }
            if mapping.get("supports_1m").and_then(Value::as_bool) == Some(true) {
                normalized.insert("supports_1m".to_string(), json!(true));
            }
            serde_json::to_string(&normalized).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    signature.sort();
    Some(signature)
}

fn source_context(
    metadata: &TransferMetadata,
    platform: PlatformId,
    kind: &str,
    issue_codes: &mut Vec<String>,
) -> (Option<CompleteSourceIdentity>, Option<ImportBatchKey>) {
    let source_identity = match (
        metadata.source_instance_id.as_ref(),
        metadata.source_credential_id.as_ref(),
    ) {
        (Some(source_instance_id), Some(source_credential_id)) => Some(CompleteSourceIdentity {
            source_instance_id: source_instance_id.clone(),
            source_credential_id: source_credential_id.clone(),
            source_platform: platform.as_str().to_string(),
            source_kind: kind.to_string(),
            source_schema_version: metadata.schema_version,
        }),
        (Some(_), None) | (None, Some(_)) => {
            push_issue_code(issue_codes, "transfer.source_identity_partial");
            None
        }
        (None, None) => None,
    };
    let batch_key = metadata
        .batch_name
        .as_ref()
        .map(|batch_name| ImportBatchKey {
            source_instance_id: metadata.source_instance_id.clone(),
            source_batch_id: metadata.source_batch_id.clone(),
            batch_name: batch_name.clone(),
        });
    (source_identity, batch_key)
}

fn official_allowlisted_raw(
    cpa_type: &str,
    email: Option<&str>,
    secret: &Map<String, Value>,
    config: &Map<String, Value>,
) -> Map<String, Value> {
    let mut raw = Map::from_iter([("type".to_string(), json!(cpa_type))]);
    if let Some(email) = email {
        raw.insert("email".to_string(), json!(email));
    }
    for (field, _) in OFFICIAL_SECRET_FIELDS {
        if let Some(value) = secret.get(*field).filter(|value| is_nonempty(value)) {
            raw.insert((*field).to_string(), value.clone());
        }
    }
    for (field, _) in OFFICIAL_CONFIG_FIELDS {
        if let Some(value) = config.get(*field).filter(|value| is_nonempty(value)) {
            raw.insert((*field).to_string(), value.clone());
        }
    }
    raw
}

fn normalize_raw_aliases(
    raw: &mut Map<String, Value>,
    canonical: &str,
    aliases: &[&str],
    source: &Map<String, Value>,
) {
    for alias in aliases {
        raw.remove(*alias);
    }
    if let Some(value) = first_value(source, aliases).filter(|value| is_nonempty(value)) {
        raw.insert(canonical.to_string(), normalized_value(value));
    }
}

fn api_fingerprint_credential(item: &Map<String, Value>, cpa_section: &str) -> Value {
    let mut credential = item.clone();
    credential.remove("x-ai-switch");
    credential.remove("type");
    if cpa_section == "openai-compatibility" {
        credential.remove("api-key");
        credential.remove("api_key");
    } else {
        credential.remove("api-key-entries");
        credential.remove("api_key_entries");
    }
    Value::Object(credential)
}

fn derive_api_section(platform: PlatformId, dialect: ApiDialect, base_url: &str) -> &'static str {
    match dialect {
        ApiDialect::Anthropic => "claude-api-key",
        ApiDialect::Gemini => "gemini-api-key",
        ApiDialect::OpenAiResponses => "codex-api-key",
        ApiDialect::OpenAi if platform == PlatformId::Grok && is_official_xai_url(base_url) => {
            "xai-api-key"
        }
        ApiDialect::OpenAi => "openai-compatibility",
    }
}

fn expected_dialect_for_section(section: &str) -> Option<ApiDialect> {
    match section {
        "claude-api-key" => Some(ApiDialect::Anthropic),
        "gemini-api-key" => Some(ApiDialect::Gemini),
        "codex-api-key" => Some(ApiDialect::OpenAiResponses),
        "xai-api-key" | "openai-compatibility" => Some(ApiDialect::OpenAi),
        _ => None,
    }
}

fn fixed_platform_for_section(section: &str) -> Option<PlatformId> {
    match section {
        "claude-api-key" => Some(PlatformId::Claude),
        "gemini-api-key" => Some(PlatformId::Gemini),
        "codex-api-key" => Some(PlatformId::Codex),
        "xai-api-key" => Some(PlatformId::Grok),
        _ => None,
    }
}

fn is_supported_api_section(section: &str) -> bool {
    matches!(
        section,
        "claude-api-key"
            | "gemini-api-key"
            | "codex-api-key"
            | "xai-api-key"
            | "openai-compatibility"
    )
}

fn is_explicitly_unsupported_section(section: &str) -> bool {
    matches!(section, "interactions-api-key" | "vertex-api-key")
}

fn is_supported_official_platform(platform: PlatformId) -> bool {
    matches!(
        platform,
        PlatformId::Codex | PlatformId::Claude | PlatformId::Gemini | PlatformId::Grok
    )
}

fn official_cpa_type(platform: PlatformId) -> &'static str {
    if platform == PlatformId::Grok {
        "xai"
    } else {
        platform.as_str()
    }
}

fn validate_api_url(value: &str) -> Result<(), &'static str> {
    let url = Url::parse(value.trim()).map_err(|_| "transfer.base_url_invalid")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("transfer.base_url_invalid");
    }
    Ok(())
}

fn is_official_xai_url(value: &str) -> bool {
    Url::parse(value.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "api.x.ai")
}

fn preliminary_display_name(item_index: usize, item: &Map<String, Value>) -> String {
    item.get("x-ai-switch")
        .and_then(Value::as_object)
        .and_then(|metadata| optional_string(metadata, "display_name"))
        .or_else(|| {
            string_value(
                item,
                &["display_name", "displayName", "name", "label", "email"],
            )
        })
        .unwrap_or_else(|| format!("Item {}", item_index + 1))
}

fn final_display_name(
    item_index: usize,
    item: &Map<String, Value>,
    metadata_display_name: Option<&str>,
) -> String {
    metadata_display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            string_value(
                item,
                &["display_name", "displayName", "name", "label", "email"],
            )
        })
        .unwrap_or_else(|| format!("Item {}", item_index + 1))
}

fn mask_display_name(value: &str) -> String {
    let characters = value.trim().chars().collect::<Vec<_>>();
    match characters.as_slice() {
        [] => "I***1".to_string(),
        [first] | [first, _] => format!("{first}*"),
        [first, .., last] => format!("{first}***{last}"),
    }
}

fn has_api_shape(item: &Map<String, Value>) -> bool {
    [
        "api-key",
        "api_key",
        "api-key-entries",
        "api_key_entries",
        "base-url",
    ]
    .iter()
    .any(|field| item.get(*field).is_some_and(is_nonempty))
}

fn has_official_auth_shape(item: &Map<String, Value>) -> bool {
    OFFICIAL_SECRET_FIELDS
        .iter()
        .any(|(_, aliases)| first_value(item, aliases).is_some_and(is_nonempty))
}

fn has_nonempty_field(object: &Map<String, Value>, field: &str) -> bool {
    object.get(field).is_some_and(is_nonempty)
}

fn first_value<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn string_value(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    first_value(object, keys)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    string_value(object, &[key])
}

fn normalized_value(value: &Value) -> Value {
    match value {
        Value::String(value) => Value::String(value.trim().to_string()),
        other => other.clone(),
    }
}

fn is_nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}

fn normalize_cpa_section(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '_'], "-")
}

fn field_is_secret_bearing(field: &str, value: &Value) -> bool {
    let normalized = field.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    let sensitive_name = normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized.contains("credential")
        || normalized.contains("authorization")
        || normalized == "key"
        || normalized.ends_with("_key")
        || normalized.contains("private_key")
        || normalized.contains("api_key");
    if sensitive_name {
        return true;
    }
    nested_value_contains_secret_field(value)
}

fn nested_value_contains_secret_field(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(child, value)| field_is_secret_bearing(child, value)),
        Value::Array(values) => values.iter().any(nested_value_contains_secret_field),
        _ => false,
    }
}

fn push_issue_code(issue_codes: &mut Vec<String>, code: &str) {
    if !issue_codes.iter().any(|existing| existing == code) {
        issue_codes.push(code.to_string());
    }
}

fn transfer_issue(
    item_index: usize,
    display_name_masked: &str,
    code: &str,
    field: Option<&str>,
) -> RouteCredentialTransferIssue {
    RouteCredentialTransferIssue {
        item_index: Some(item_index),
        display_name: Some(display_name_masked.to_string()),
        code: code.to_string(),
        field: field.map(str::to_string),
    }
}

fn validation_error(code: &'static str, message: &str) -> AppError {
    AppError::Validation {
        code,
        message: message.to_string(),
        details: None,
        recoverable: true,
    }
}
