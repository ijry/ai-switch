use super::{
    FileLogStore, LogConfig, LogPage, LogQuery, LogQueue, LogQueueKind, LogRecord, LogStatus,
    LogStore, LogStoreKind, MemoryLogQueue,
};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::time::{timeout, timeout_at, Instant};

#[derive(Clone, Default)]
pub struct LogRuntime {
    inner: Arc<RuntimeInner>,
}

#[derive(Default)]
struct RuntimeInner {
    transition: Mutex<()>,
    active: RwLock<Option<Arc<Generation>>>,
}

struct Generation {
    config: LogConfig,
    queue: Arc<dyn LogQueue>,
    store: Arc<dyn LogStore>,
    accepting: RwLock<bool>,
    draining: AtomicBool,
    completed: AtomicBool,
    wake: Notify,
    drained: Notify,
    status: Mutex<LogStatus>,
}

impl LogRuntime {
    pub async fn configure(&self, config: LogConfig) -> Result<(), String> {
        config.validate()?;
        let _transition = self.inner.transition.lock().await;
        let store: Arc<dyn LogStore> = match config.store {
            LogStoreKind::File => {
                let root = match &config.directory {
                    Some(path) => path.clone(),
                    None => directories::BaseDirs::new()
                        .ok_or("home directory unavailable")?
                        .home_dir()
                        .join("ai-switch/logs"),
                };
                Arc::new(FileLogStore::open(root).await?)
            }
            LogStoreKind::Postgres => Arc::new(super::PostgresLogStore::connect(&config).await?),
        };
        let queue: Arc<dyn LogQueue> = match config.queue {
            LogQueueKind::Memory => {
                Arc::new(MemoryLogQueue::new(config.max_records, config.max_bytes))
            }
            LogQueueKind::Redis => Arc::new(super::RedisLogQueue::connect(&config).await?),
        };
        if let Some(active) = self.inner.active.read().await.clone() {
            active.drain().await?;
        }
        let generation = Arc::new(Generation {
            status: Mutex::new(LogStatus {
                configured: true,
                accepting: true,
                queue: config.queue,
                store: config.store,
                ..LogStatus::default()
            }),
            config,
            queue,
            store,
            accepting: RwLock::new(true),
            draining: AtomicBool::new(false),
            completed: AtomicBool::new(false),
            wake: Notify::new(),
            drained: Notify::new(),
        });
        *self.inner.active.write().await = Some(generation.clone());
        tokio::spawn(generation.run());
        Ok(())
    }

    pub async fn enqueue(&self, record: LogRecord) -> Result<(), String> {
        let active = self.active().await?;
        let admission = active.accepting.read().await;
        if !*admission {
            return Err("request log runtime is draining".into());
        }
        let record = record.sanitized()?;
        let deadline = Instant::now() + Duration::from_millis(active.config.enqueue_timeout_ms);
        loop {
            let accepted = timeout_at(deadline, active.queue.try_enqueue(record.clone())).await;
            match accepted {
                Ok(Ok(true)) => {
                    active.wake.notify_one();
                    return Ok(());
                }
                Ok(Ok(false)) => {}
                Ok(Err(_)) | Err(_) => {
                    active.failure("request log queue unavailable").await;
                    return Err("request log queue unavailable".into());
                }
            }
            if Instant::now() >= deadline {
                return Err("request log queue capacity exhausted".into());
            }
            tokio::time::sleep_until(deadline.min(Instant::now() + Duration::from_millis(10)))
                .await;
        }
    }

    pub async fn query(&self, query: LogQuery) -> Result<LogPage, String> {
        let active = self.active().await?;
        let result = active.store.query(query).await?;
        active.status.lock().await.malformed_lines = result.malformed_lines;
        Ok(result)
    }

    pub async fn status(&self) -> Result<LogStatus, String> {
        let Some(active) = self.inner.active.read().await.clone() else {
            return Ok(LogStatus::default());
        };
        let accepting = *active.accepting.read().await;
        let stats = active.queue.stats().await;
        let mut status = active.status.lock().await;
        status.accepting = accepting;
        match stats {
            Ok(stats) => {
                status.pending = stats.pending;
                status.pending_bytes = stats.pending_bytes;
                status.accepting &= stats.pending < active.config.max_records
                    && stats.pending_bytes < active.config.max_bytes;
            }
            Err(_) => {
                status.accepting = false;
                status.last_error = Some("request log queue status unavailable".into());
            }
        }
        Ok(status.clone())
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        let _transition = self.inner.transition.lock().await;
        if let Some(active) = self.inner.active.read().await.clone() {
            active.drain().await?;
        }
        Ok(())
    }

    async fn active(&self) -> Result<Arc<Generation>, String> {
        self.inner
            .active
            .read()
            .await
            .clone()
            .ok_or_else(|| "request log runtime is not configured".into())
    }
}

impl Generation {
    async fn drain(&self) -> Result<(), String> {
        *self.accepting.write().await = false;
        self.draining.store(true, Ordering::Release);
        self.wake.notify_one();
        let result = timeout(
            Duration::from_millis(self.config.shutdown_timeout_ms),
            async {
                loop {
                    let notified = self.drained.notified();
                    if self.completed.load(Ordering::Acquire) {
                        break;
                    }
                    notified.await;
                }
            },
        )
        .await;
        if result.is_err() {
            let stats = self.queue.stats().await.ok();
            let mut status = self.status.lock().await;
            status.shutdown_pending = stats.as_ref().map_or(status.pending, |stats| stats.pending);
            status.last_error =
                Some("request log drain timed out; unacknowledged logs retained".into());
            return Err(format!(
                "request log drain timed out; {} records remain (consumer continues retrying)",
                status.shutdown_pending
            ));
        }
        self.status.lock().await.shutdown_pending = 0;
        Ok(())
    }

    async fn failure(&self, safe_error: &str) {
        let mut status = self.status.lock().await;
        status.consecutive_failures = status.consecutive_failures.saturating_add(1);
        status.retries = status.retries.saturating_add(1);
        status.last_error = Some(safe_error.into());
    }

    async fn run(self: Arc<Self>) {
        let mut retry_ms = 50;
        let mut next_retention = Instant::now() + Duration::from_secs(60);
        loop {
            let result = self.deliver().await;
            match result {
                Ok(false) if self.draining.load(Ordering::Acquire) => {
                    match self.queue.stats().await {
                        Ok(stats) if stats.pending == 0 => {
                            self.completed.store(true, Ordering::Release);
                            self.drained.notify_waiters();
                            break;
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            self.failure("request log drain status unavailable").await;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                }
                Ok(wrote) => {
                    retry_ms = 50;
                    if !self.draining.load(Ordering::Acquire) && Instant::now() >= next_retention {
                        if self
                            .store
                            .retention(self.config.retention_days)
                            .await
                            .is_err()
                        {
                            self.failure("request log retention failed").await;
                        }
                        next_retention = Instant::now() + Duration::from_secs(3600);
                    }
                    if !wrote {
                        tokio::select! { _ = self.wake.notified() => {}, _ = tokio::time::sleep(Duration::from_millis(200)) => {} }
                    }
                }
                Err(error) => {
                    self.failure(error).await;
                    tokio::time::sleep(Duration::from_millis(retry_ms)).await;
                    retry_ms = (retry_ms * 2).min(5_000);
                }
            }
        }
    }

    async fn deliver(&self) -> Result<bool, &'static str> {
        let batch = self
            .queue
            .receive(self.config.batch_size)
            .await
            .map_err(|_| "request log queue receive failed")?;
        if batch.records.is_empty() {
            return Ok(false);
        }
        self.store
            .append_batch(&batch.records)
            .await
            .map_err(|_| "request log store write failed")?;
        self.queue
            .ack(&batch)
            .await
            .map_err(|_| "request log queue acknowledgement failed")?;
        let mut status = self.status.lock().await;
        status.last_success_at = Some(Utc::now());
        status.consecutive_failures = 0;
        status.last_error = None;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{QueueBatch, QueueStats};
    use super::*;
    use async_trait::async_trait;

    struct PausedQueue {
        memory: MemoryLogQueue,
        empty_seen: Notify,
        release: Notify,
        paused: AtomicBool,
        stall_enqueue: bool,
    }

    #[async_trait]
    impl LogQueue for PausedQueue {
        async fn try_enqueue(&self, record: LogRecord) -> Result<bool, String> {
            if self.stall_enqueue {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            self.memory.try_enqueue(record).await
        }
        async fn receive(&self, maximum: usize) -> Result<QueueBatch, String> {
            let batch = self.memory.receive(maximum).await?;
            if batch.records.is_empty() && !self.paused.swap(true, Ordering::AcqRel) {
                self.empty_seen.notify_one();
                self.release.notified().await;
            }
            Ok(batch)
        }
        async fn ack(&self, batch: &QueueBatch) -> Result<(), String> {
            self.memory.ack(batch).await
        }
        async fn stats(&self) -> Result<QueueStats, String> {
            self.memory.stats().await
        }
    }

    fn generation(queue: Arc<dyn LogQueue>, store: Arc<dyn LogStore>) -> Arc<Generation> {
        Arc::new(Generation {
            config: LogConfig {
                enqueue_timeout_ms: 20,
                ..LogConfig::default()
            },
            queue,
            store,
            accepting: RwLock::new(true),
            draining: AtomicBool::new(false),
            completed: AtomicBool::new(false),
            wake: Notify::new(),
            drained: Notify::new(),
            status: Mutex::new(LogStatus::default()),
        })
    }

    #[tokio::test]
    async fn shutdown_does_not_mistake_a_stale_empty_read_for_a_drained_queue() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            FileLogStore::open(directory.path().join("logs"))
                .await
                .unwrap(),
        );
        let queue = Arc::new(PausedQueue {
            memory: MemoryLogQueue::new(10, 100_000),
            empty_seen: Notify::new(),
            release: Notify::new(),
            paused: AtomicBool::new(false),
            stall_enqueue: false,
        });
        let active = generation(queue.clone(), store.clone());
        tokio::spawn(active.clone().run());
        queue.empty_seen.notified().await;
        queue
            .try_enqueue(super::super::tests::record("drain-race", "owner"))
            .await
            .unwrap();
        let draining = active.clone();
        let task = tokio::spawn(async move { draining.drain().await });
        while !active.draining.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        queue.release.notify_one();
        task.await.unwrap().unwrap();
        assert_eq!(queue.stats().await.unwrap().pending, 0);
        assert_eq!(
            store
                .query(LogQuery::for_user("owner"))
                .await
                .unwrap()
                .total,
            1
        );
    }

    #[tokio::test]
    async fn enqueue_deadline_includes_slow_external_queue_operations() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            FileLogStore::open(directory.path().join("logs"))
                .await
                .unwrap(),
        );
        let queue = Arc::new(PausedQueue {
            memory: MemoryLogQueue::new(10, 100_000),
            empty_seen: Notify::new(),
            release: Notify::new(),
            paused: AtomicBool::new(false),
            stall_enqueue: true,
        });
        let runtime = LogRuntime::default();
        *runtime.inner.active.write().await = Some(generation(queue, store));
        let result = timeout(
            Duration::from_millis(200),
            runtime.enqueue(super::super::tests::record("deadline", "owner")),
        )
        .await;
        assert!(
            result.is_ok(),
            "enqueue must not outlive its configured admission deadline"
        );
        assert!(result.unwrap().is_err());
    }
}
