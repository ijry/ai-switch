use super::LogRecord;
use async_trait::async_trait;
use std::collections::VecDeque;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Default)]
pub struct QueueStats {
    pub pending: usize,
    pub pending_bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct QueueBatch {
    pub receipts: Vec<String>,
    pub records: Vec<LogRecord>,
}

#[async_trait]
pub trait LogQueue: Send + Sync {
    async fn try_enqueue(&self, record: LogRecord) -> Result<bool, String>;
    async fn receive(&self, maximum: usize) -> Result<QueueBatch, String>;
    async fn ack(&self, batch: &QueueBatch) -> Result<(), String>;
    async fn stats(&self) -> Result<QueueStats, String>;
}

pub struct MemoryLogQueue {
    max_records: usize,
    max_bytes: usize,
    state: Mutex<MemoryState>,
}

#[derive(Default)]
struct MemoryState {
    records: VecDeque<(String, LogRecord, usize)>,
    bytes: usize,
    sequence: u64,
}

impl MemoryLogQueue {
    pub fn new(max_records: usize, max_bytes: usize) -> Self {
        Self {
            max_records,
            max_bytes,
            state: Mutex::new(MemoryState::default()),
        }
    }
}

#[async_trait]
impl LogQueue for MemoryLogQueue {
    async fn try_enqueue(&self, record: LogRecord) -> Result<bool, String> {
        let record = record.sanitized()?;
        let bytes = serde_json::to_vec(&record)
            .map_err(|_| "log serialization failed")?
            .len();
        if bytes > self.max_bytes {
            return Err("request log exceeds queue byte capacity".into());
        }
        let mut state = self.state.lock().await;
        if state.records.len() >= self.max_records
            || state.bytes.saturating_add(bytes) > self.max_bytes
        {
            return Ok(false);
        }
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or("log queue sequence exhausted")?;
        let receipt = state.sequence.to_string();
        state.records.push_back((receipt, record, bytes));
        state.bytes += bytes;
        Ok(true)
    }
    async fn receive(&self, maximum: usize) -> Result<QueueBatch, String> {
        if maximum == 0 || maximum > 500 {
            return Err("invalid log batch size".into());
        }
        let state = self.state.lock().await;
        let mut batch = QueueBatch::default();
        for (receipt, record, _) in state.records.iter().take(maximum) {
            batch.receipts.push(receipt.clone());
            batch.records.push(record.clone());
        }
        Ok(batch)
    }
    async fn ack(&self, batch: &QueueBatch) -> Result<(), String> {
        let mut state = self.state.lock().await;
        for receipt in &batch.receipts {
            if state
                .records
                .front()
                .is_some_and(|entry| &entry.0 == receipt)
            {
                if let Some((_, _, bytes)) = state.records.pop_front() {
                    state.bytes -= bytes;
                }
            }
        }
        Ok(())
    }
    async fn stats(&self) -> Result<QueueStats, String> {
        let state = self.state.lock().await;
        Ok(QueueStats {
            pending: state.records.len(),
            pending_bytes: state.bytes,
        })
    }
}
