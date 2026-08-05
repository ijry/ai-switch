use crate::models::route_credential::RouteCredentialPoolScope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TRANSFER_FORMAT: &str = "ai-switch.route-credential";
pub const TRANSFER_SCHEMA_VERSION: u32 = 1;
pub const TRANSFER_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const TRANSFER_MAX_ITEMS: usize = 2_000;
pub const TRANSFER_MAX_ITEM_BYTES: usize = 256 * 1024;
pub const TRANSFER_MAX_EXPORT_IDS: usize = 2_000;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialSelectionContext {
    pub platform: String,
    pub pool_scope: RouteCredentialPoolScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportRouteCredentialsInput {
    pub selection_context: RouteCredentialSelectionContext,
    pub credential_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub include_enhanced_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferPlatformChoice {
    pub item_index: usize,
    pub platform: String,
    #[serde(default)]
    pub interface_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialTransferIssue {
    pub item_index: Option<usize>,
    pub display_name: Option<String>,
    pub code: String,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialExportCounts {
    pub total: usize,
    pub official: usize,
    pub api: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialSchemeLink {
    pub credential_id: String,
    pub display_name: String,
    pub url: Option<String>,
    pub issue_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialExportResult {
    pub json_text: Option<String>,
    pub suggested_file_name: String,
    pub counts: RouteCredentialExportCounts,
    pub scheme_links: Vec<RouteCredentialSchemeLink>,
    pub warnings: Vec<RouteCredentialTransferIssue>,
    pub errors: Vec<RouteCredentialTransferIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewRouteCredentialImportInput {
    pub text: String,
    #[serde(default)]
    pub ambiguous_platform_choices: Vec<TransferPlatformChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportPreviewItem {
    pub item_index: usize,
    pub display_name_masked: String,
    pub platform: Option<String>,
    pub kind: Option<String>,
    pub cpa_section: Option<String>,
    pub disposition: String,
    pub issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialImportPreviewCounts {
    pub total: usize,
    pub official: usize,
    pub api: usize,
    pub importable: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub errors: usize,
    pub restorable_pool_count: usize,
    pub batch_count: usize,
    pub platform_counts: BTreeMap<String, usize>,
    pub cpa_section_counts: BTreeMap<String, usize>,
    pub legacy_type_counts: BTreeMap<String, usize>,
    pub restorable_pool_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteCredentialImportPreview {
    pub counts: RouteCredentialImportPreviewCounts,
    pub items: Vec<RouteCredentialImportPreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportRouteCredentialsInput {
    pub text: String,
    #[serde(default)]
    pub ambiguous_platform_choices: Vec<TransferPlatformChoice>,
    #[serde(default)]
    pub restore_pool_membership: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RouteCredentialImportOutcome {
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub restored_pool_members: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    fn assert_exact_json<T: Serialize>(value: &T, expected: Value) {
        assert_eq!(
            serde_json::to_value(value).expect("serialize DTO"),
            expected
        );
    }

    fn assert_transport_value_is_redacted(value: &Value) {
        const FORBIDDEN_KEYS: [&str; 6] = [
            "secret_payload_json",
            "config_json",
            "api_key",
            "access_token",
            "refresh_token",
            "fingerprint",
        ];

        match value {
            Value::Object(object) => {
                for key in FORBIDDEN_KEYS {
                    assert!(!object.contains_key(key), "transport DTO exposed {key}");
                }
                for child in object.values() {
                    assert_transport_value_is_redacted(child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    assert_transport_value_is_redacted(item);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn route_credential_transfer_constants_match_contract() {
        assert_eq!(TRANSFER_FORMAT, "ai-switch.route-credential");
        assert_eq!(TRANSFER_SCHEMA_VERSION, 1);
        assert_eq!(TRANSFER_MAX_BYTES, 8 * 1024 * 1024);
        assert_eq!(TRANSFER_MAX_ITEMS, 2_000);
        assert_eq!(TRANSFER_MAX_ITEM_BYTES, 256 * 1024);
        assert_eq!(TRANSFER_MAX_EXPORT_IDS, 2_000);
    }

    #[test]
    fn export_input_uses_snake_case_and_defaults_enhanced_metadata() {
        let input: ExportRouteCredentialsInput = serde_json::from_value(json!({
            "selection_context": {
                "platform": "codex",
                "pool_scope": "in_pool"
            },
            "credential_ids": ["credential-1"]
        }))
        .expect("deserialize export input");

        assert!(input.include_enhanced_metadata);
        assert_exact_json(
            &input,
            json!({
                "selection_context": {
                    "platform": "codex",
                    "pool_scope": "in_pool"
                },
                "credential_ids": ["credential-1"],
                "include_enhanced_metadata": true
            }),
        );

        let disabled: ExportRouteCredentialsInput = serde_json::from_value(json!({
            "selection_context": {
                "platform": "codex",
                "pool_scope": "out_of_pool"
            },
            "credential_ids": ["credential-2"],
            "include_enhanced_metadata": false
        }))
        .expect("deserialize disabled enhanced metadata");
        assert!(!disabled.include_enhanced_metadata);
        assert_exact_json(
            &disabled,
            json!({
                "selection_context": {
                    "platform": "codex",
                    "pool_scope": "out_of_pool"
                },
                "credential_ids": ["credential-2"],
                "include_enhanced_metadata": false
            }),
        );
    }

    #[test]
    fn export_result_dtos_use_exact_transport_names() {
        let explicit_export_content = r#"[{"api_key":"sk-test","x-ai-switch":{"format":"ai-switch.route-credential","schema_version":1,"source_instance_id":"installation-uuid","source_credential_id":"credential-2","platform":"claude","kind":"api"}}]"#;
        let result = RouteCredentialExportResult {
            json_text: Some(explicit_export_content.to_string()),
            suggested_file_name: "ai-switch-route-credentials.json".to_string(),
            counts: RouteCredentialExportCounts {
                total: 2,
                official: 1,
                api: 1,
            },
            scheme_links: vec![RouteCredentialSchemeLink {
                credential_id: "credential-2".to_string(),
                display_name: "API route".to_string(),
                url: Some("claude-code-router://import?data=explicit-export-content".to_string()),
                issue_code: None,
            }],
            warnings: vec![RouteCredentialTransferIssue {
                item_index: Some(1),
                display_name: Some("API route".to_string()),
                code: "transfer.scheme_unavailable".to_string(),
                field: Some("interface_format".to_string()),
            }],
            errors: Vec::new(),
        };

        assert_exact_json(
            &result,
            json!({
                "json_text": explicit_export_content,
                "suggested_file_name": "ai-switch-route-credentials.json",
                "counts": {"total": 2, "official": 1, "api": 1},
                "scheme_links": [{
                    "credential_id": "credential-2",
                    "display_name": "API route",
                    "url": "claude-code-router://import?data=explicit-export-content",
                    "issue_code": null
                }],
                "warnings": [{
                    "item_index": 1,
                    "display_name": "API route",
                    "code": "transfer.scheme_unavailable",
                    "field": "interface_format"
                }],
                "errors": []
            }),
        );

        assert_transport_value_is_redacted(
            &serde_json::to_value(result).expect("serialize result"),
        );
    }

    #[test]
    fn import_transport_dtos_preserve_structured_choices_and_masked_names() {
        let choice = TransferPlatformChoice {
            item_index: 3,
            platform: "claude".to_string(),
            interface_format: Some("anthropic-messages".to_string()),
        };
        let preview_input = PreviewRouteCredentialImportInput {
            text: "explicit export content".to_string(),
            ambiguous_platform_choices: vec![choice.clone()],
        };
        let preview = RouteCredentialImportPreview {
            counts: RouteCredentialImportPreviewCounts {
                total: 1,
                official: 0,
                api: 1,
                importable: 1,
                duplicates: 0,
                conflicts: 0,
                errors: 0,
                restorable_pool_count: 1,
                batch_count: 1,
                platform_counts: BTreeMap::from([("claude".to_string(), 1)]),
                cpa_section_counts: BTreeMap::from([("claude".to_string(), 1)]),
                legacy_type_counts: BTreeMap::new(),
                restorable_pool_counts: BTreeMap::from([("claude".to_string(), 1)]),
            },
            items: vec![RouteCredentialImportPreviewItem {
                item_index: 3,
                display_name_masked: "Cl***te".to_string(),
                platform: Some("claude".to_string()),
                kind: Some("api".to_string()),
                cpa_section: Some("claude".to_string()),
                disposition: "importable".to_string(),
                issue_codes: Vec::new(),
            }],
        };
        let import_input = ImportRouteCredentialsInput {
            text: "explicit export content".to_string(),
            ambiguous_platform_choices: vec![choice],
            restore_pool_membership: true,
        };
        let outcome = RouteCredentialImportOutcome {
            imported: 1,
            skipped_duplicates: 2,
            conflicts: 3,
            failed: 4,
            restored_pool_members: 1,
        };

        assert_exact_json(
            &preview_input,
            json!({
                "text": "explicit export content",
                "ambiguous_platform_choices": [{
                    "item_index": 3,
                    "platform": "claude",
                    "interface_format": "anthropic-messages"
                }]
            }),
        );
        assert_exact_json(
            &preview,
            json!({
                "counts": {
                    "total": 1, "official": 0, "api": 1, "importable": 1,
                    "duplicates": 0, "conflicts": 0, "errors": 0,
                    "restorable_pool_count": 1, "batch_count": 1,
                    "platform_counts": {"claude": 1},
                    "cpa_section_counts": {"claude": 1},
                    "legacy_type_counts": {},
                    "restorable_pool_counts": {"claude": 1}
                },
                "items": [{
                    "item_index": 3,
                    "display_name_masked": "Cl***te",
                    "platform": "claude",
                    "kind": "api",
                    "cpa_section": "claude",
                    "disposition": "importable",
                    "issue_codes": []
                }]
            }),
        );
        assert_exact_json(
            &import_input,
            json!({
                "text": "explicit export content",
                "ambiguous_platform_choices": [{
                    "item_index": 3,
                    "platform": "claude",
                    "interface_format": "anthropic-messages"
                }],
                "restore_pool_membership": true
            }),
        );
        assert_exact_json(
            &outcome,
            json!({
                "imported": 1,
                "skipped_duplicates": 2,
                "conflicts": 3,
                "failed": 4,
                "restored_pool_members": 1
            }),
        );

        for value in [
            serde_json::to_value(preview_input).expect("serialize preview input"),
            serde_json::to_value(preview).expect("serialize preview"),
            serde_json::to_value(import_input).expect("serialize import input"),
            serde_json::to_value(outcome).expect("serialize outcome"),
        ] {
            assert_transport_value_is_redacted(&value);
        }
    }

    #[test]
    fn import_inputs_default_optional_controls() {
        let preview: PreviewRouteCredentialImportInput =
            serde_json::from_value(json!({"text": "content"})).expect("preview input");
        let import: ImportRouteCredentialsInput =
            serde_json::from_value(json!({"text": "content"})).expect("import input");

        assert!(preview.ambiguous_platform_choices.is_empty());
        assert!(import.ambiguous_platform_choices.is_empty());
        assert!(!import.restore_pool_membership);
    }

    #[test]
    fn selection_context_serializes_existing_pool_scope_enum() {
        assert_exact_json(
            &RouteCredentialSelectionContext {
                platform: "grok".to_string(),
                pool_scope: RouteCredentialPoolScope::OutOfPool,
            },
            json!({"platform": "grok", "pool_scope": "out_of_pool"}),
        );
    }
}
