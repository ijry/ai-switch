use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogQueueKind {
    #[default]
    Memory,
    Redis,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogStoreKind {
    #[default]
    File,
    Postgres,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LogConfig {
    pub queue: LogQueueKind,
    pub store: LogStoreKind,
    pub directory: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub redis_url: Option<String>,
    #[serde(skip_serializing)]
    pub postgres_url: Option<String>,
    pub instance_id: String,
    pub max_records: usize,
    pub max_bytes: usize,
    pub enqueue_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub batch_size: usize,
    pub retention_days: Option<u32>,
    pub postgres_max_connections: u32,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            queue: LogQueueKind::Memory,
            store: LogStoreKind::File,
            directory: None,
            redis_url: None,
            postgres_url: None,
            instance_id: "default".into(),
            max_records: 10_000,
            max_bytes: 16 * 1024 * 1024,
            enqueue_timeout_ms: 100,
            shutdown_timeout_ms: 10_000,
            batch_size: 100,
            retention_days: Some(30),
            postgres_max_connections: 5,
        }
    }
}

impl std::fmt::Debug for LogConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LogConfig")
            .field("queue", &self.queue)
            .field("store", &self.store)
            .field("credentials", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LogRecord {
    pub schema_version: u32,
    pub request_id: String,
    pub user_id: String,
    pub key_id: String,
    pub group_id: String,
    pub platform: String,
    pub model: String,
    pub endpoint: String,
    pub created_at: DateTime<Utc>,
    pub utc_offset_minutes: i32,
    pub duration_ms: i64,
    pub status: u16,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub amount_usd_micros: i64,
    pub settlement_status: String,
    pub error_code: Option<String>,
}

impl Default for LogRecord {
    fn default() -> Self {
        Self {
            schema_version: 1,
            request_id: String::new(),
            user_id: String::new(),
            key_id: String::new(),
            group_id: String::new(),
            platform: String::new(),
            model: String::new(),
            endpoint: String::new(),
            created_at: Utc::now(),
            utc_offset_minutes: Local::now().offset().local_minus_utc() / 60,
            duration_ms: 0,
            status: 0,
            input_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 0,
            amount_usd_micros: 0,
            settlement_status: "unknown".into(),
            error_code: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LogQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub user_id: Option<String>,
    pub key_id: Option<String>,
    pub group_id: Option<String>,
    pub platform: Option<String>,
    pub model: Option<String>,
    pub status: Option<u16>,
    pub page: u32,
    pub page_size: u32,
    #[serde(skip)]
    pub owner_user_id: Option<String>,
}

impl Default for LogQuery {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            user_id: None,
            key_id: None,
            group_id: None,
            platform: None,
            model: None,
            status: None,
            page: 1,
            page_size: 50,
            owner_user_id: None,
        }
    }
}

impl LogQuery {
    pub fn for_user(user_id: impl Into<String>) -> Self {
        Self {
            owner_user_id: Some(user_id.into()),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub items: Vec<LogRecord>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub malformed_lines: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub configured: bool,
    pub accepting: bool,
    pub queue: LogQueueKind,
    pub store: LogStoreKind,
    pub pending: usize,
    pub pending_bytes: usize,
    pub last_success_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u64,
    pub retries: u64,
    pub shutdown_pending: usize,
    pub malformed_lines: u64,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait LogStore: Send + Sync {
    async fn append_batch(&self, records: &[LogRecord]) -> Result<(), String>;
    async fn query(&self, query: LogQuery) -> Result<LogPage, String>;
    async fn retention(&self, days: Option<u32>) -> Result<u64, String>;
}

pub(crate) const MAX_RECORD_BYTES: usize = 16 * 1024;

impl LogConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_records == 0
            || self.max_records > 1_000_000
            || self.max_bytes < 1024
            || self.max_bytes > 1024 * 1024 * 1024
            || self.batch_size == 0
            || self.batch_size > 500
            || self.batch_size > self.max_records
            || self.enqueue_timeout_ms > 30_000
            || self.shutdown_timeout_ms == 0
            || self.shutdown_timeout_ms > 300_000
            || self.postgres_max_connections == 0
            || self.postgres_max_connections > 50
            || self
                .retention_days
                .is_some_and(|days| days == 0 || days > 3650)
        {
            return Err("invalid log capacity, timeout, pool or retention limit".into());
        }
        if self.instance_id.is_empty()
            || self.instance_id.len() > 100
            || !self
                .instance_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            || (self.queue == LogQueueKind::Redis && self.instance_id == "default")
        {
            return Err("a stable, unique log instance ID is required for Redis".into());
        }
        Ok(())
    }
}

impl LogRecord {
    pub(crate) fn sanitized(&self) -> Result<Self, String> {
        let mut record = self.clone();
        if record.schema_version != 1
            || [
                &record.request_id,
                &record.user_id,
                &record.key_id,
                &record.group_id,
            ]
            .iter()
            .any(|value| !safe_label(value, 128))
            || [
                record.duration_ms,
                record.input_tokens,
                record.cache_read_tokens,
                record.cache_write_tokens,
                record.output_tokens,
                record.amount_usd_micros,
            ]
            .iter()
            .any(|value| *value < 0)
            || record.status > 599
            || !(1970..=9999).contains(&record.created_at.year())
            || !(-1440..=1440).contains(&record.utc_offset_minutes)
        {
            return Err("invalid request log metadata".into());
        }
        if !safe_label(&record.model, 200) {
            record.model = "[redacted]".into();
        }
        if !safe_label(&record.platform, 64) {
            record.platform = "[redacted]".into();
        }
        if !matches!(
            record.endpoint.as_str(),
            "/v1/responses"
                | "/v1/chat/completions"
                | "/v1/messages"
                | "/v1/embeddings"
                | "/v1/completions"
                | "/responses"
                | "/chat/completions"
                | "/messages"
                | "/embeddings"
                | "/completions"
                | "/other"
        ) {
            record.endpoint = "/other".into();
        }
        if !safe_label(&record.settlement_status, 40) {
            record.settlement_status = "unknown".into();
        }
        if record
            .error_code
            .as_ref()
            .is_some_and(|value| !safe_label(value, 64))
        {
            record.error_code = Some("REDACTED_ERROR".into());
        }
        if serde_json::to_vec(&record)
            .map_err(|_| "log serialization failed")?
            .len()
            > MAX_RECORD_BYTES
        {
            return Err("request log exceeds record size limit".into());
        }
        Ok(record)
    }
}

fn safe_label(value: &str, maximum: usize) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
        && ![
            "bearer",
            "authorization",
            "cookie",
            "sk-",
            "gho_",
            "ghp_",
            "github_pat_",
        ]
        .iter()
        .any(|secret| lower.contains(secret))
}

impl LogQuery {
    pub(crate) fn normalized(mut self) -> Result<Self, String> {
        let to = self.to.unwrap_or_else(Utc::now);
        let from = self.from.unwrap_or(to - Duration::hours(24));
        if from >= to
            || to - from > Duration::days(31)
            || !(1970..=9999).contains(&from.year())
            || !(1970..=9999).contains(&to.year())
            || self.page == 0
            || self.page_size == 0
            || self.page_size > 100
            || u64::from(self.page) * u64::from(self.page_size) > 100_000
        {
            return Err(
                "log query requires a valid window up to 31 days and a bounded page".into(),
            );
        }
        if let Some(owner) = &self.owner_user_id {
            if !safe_label(owner, 128) || self.user_id.as_ref().is_some_and(|user| user != owner) {
                return Err("log query owner mismatch".into());
            }
            self.user_id = Some(owner.clone());
        }
        if [
            &self.user_id,
            &self.key_id,
            &self.group_id,
            &self.platform,
            &self.model,
        ]
        .iter()
        .any(|value| value.as_ref().is_some_and(|value| value.len() > 200))
        {
            return Err("log query filter is too long".into());
        }
        self.from = Some(from);
        self.to = Some(to);
        Ok(self)
    }

    pub(crate) fn matches(&self, record: &LogRecord) -> bool {
        self.from.is_some_and(|from| record.created_at >= from)
            && self.to.is_some_and(|to| record.created_at < to)
            && self
                .user_id
                .as_ref()
                .is_none_or(|value| value == &record.user_id)
            && self
                .key_id
                .as_ref()
                .is_none_or(|value| value == &record.key_id)
            && self
                .group_id
                .as_ref()
                .is_none_or(|value| value == &record.group_id)
            && self
                .platform
                .as_ref()
                .is_none_or(|value| value == &record.platform)
            && self
                .model
                .as_ref()
                .is_none_or(|value| value == &record.model)
            && self.status.is_none_or(|value| value == record.status)
    }
}
