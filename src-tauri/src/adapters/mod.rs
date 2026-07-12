#![allow(dead_code)]

use crate::config_writer::WriteOutcome;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterWriteRequest {
    pub target_key: String,
    pub item_type: String,
    pub item_id: String,
    pub rendered_config: String,
    pub target_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterWriteResult {
    pub restart_required: bool,
    pub outcome: WriteOutcome,
}

pub trait TargetAdapter: Send + Sync {
    fn key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn restart_required(&self) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshotDraft {
    pub owner_type: String,
    pub owner_id: String,
    pub status: String,
    pub remaining_label: Option<String>,
    pub reset_at: Option<String>,
    pub summary_json: String,
    pub raw_excerpt_json: String,
}

pub trait QuotaProvider: Send + Sync {
    fn provider_key(&self) -> &'static str;
    fn describe_owner(&self, owner_id: &str) -> Result<String, AppError>;
}

pub struct MockTargetAdapter;

impl TargetAdapter for MockTargetAdapter {
    fn key(&self) -> &'static str {
        "mock"
    }

    fn display_name(&self) -> &'static str {
        "Mock Adapter"
    }

    fn restart_required(&self) -> bool {
        false
    }
}
