use crate::models::route_credential::RouteCredential;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Client id for cc-switch imports.
///
/// This exact string is stored in `route_credentials.external_source_client`, so
/// renaming it orphans every account already imported under the old value and a
/// re-import would create duplicates instead of overwriting.
pub const EXTERNAL_CLIENT_CC_SWITCH: &str = "cc-switch";

/// Upper bound on how many provider entries one external config may contribute.
/// Mirrors the transfer-import ceiling: a pathological file should be refused,
/// not streamed into the account list.
pub const EXTERNAL_CLIENT_MAX_ITEMS: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewExternalClientImportInput {
    pub client: String,
    pub platform: String,
    /// Explicit config file chosen by the user. `None` means "look in the
    /// client's default location".
    #[serde(default)]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportExternalClientAccountsInput {
    pub client: String,
    pub platform: String,
    #[serde(default)]
    pub source_path: Option<String>,
    /// The source's own record ids, exactly as reported by the preview. The
    /// import re-reads the config rather than trusting anything but these ids,
    /// so secrets never round-trip through the frontend.
    pub source_ids: Vec<String>,
}

/// One provider entry found in the external client's config.
///
/// Secret-bearing fields are masked: the preview is a display payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalClientAccountPreviewItem {
    pub source_id: String,
    pub display_name: String,
    pub platform: String,
    pub interface_format: Option<String>,
    pub base_url: Option<String>,
    pub api_key_masked: Option<String>,
    pub model_mapping_count: usize,
    /// `create`, `overwrite`, or `error`.
    pub disposition: String,
    pub existing_credential_id: Option<String>,
    pub existing_display_name: Option<String>,
    pub issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExternalClientImportPreviewCounts {
    pub total: usize,
    pub importable: usize,
    pub create: usize,
    pub overwrite: usize,
    pub errors: usize,
    /// Entries that belong to a different AI Switch platform than the one being
    /// previewed. Reported so the list not showing them reads as intentional.
    pub other_platform: usize,
    pub other_platform_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalClientImportPreview {
    pub client: String,
    pub source_path: String,
    pub counts: ExternalClientImportPreviewCounts,
    pub items: Vec<ExternalClientAccountPreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExternalClientImportOutcome {
    pub created: usize,
    pub overwritten: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Every row written, created and overwritten alike, so the caller can
    /// refresh its cache from one payload.
    pub imported: Vec<RouteCredential>,
    /// Ids of the rows that did not exist before. Pool membership is offered for
    /// these only — an overwrite must not silently move an existing account in
    /// or out of the compute pool.
    pub created_ids: Vec<String>,
}
