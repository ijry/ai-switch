use super::*;
use chrono::{Duration, Local, TimeZone, Utc};
use std::io::Write;

pub(super) fn record(request_id: &str, user_id: &str) -> LogRecord {
    LogRecord {
        request_id: request_id.into(),
        user_id: user_id.into(),
        key_id: "key-1".into(),
        group_id: "group-1".into(),
        platform: "openai".into(),
        model: "gpt-test".into(),
        endpoint: "/v1/responses".into(),
        created_at: Utc::now(),
        status: 200,
        input_tokens: 13,
        output_tokens: 7,
        amount_usd_micros: 123,
        settlement_status: "settled".into(),
        ..LogRecord::default()
    }
}

#[test]
fn log_test_directory_uses_a_canonical_temp_root() {
    let directory = canonical_test_directory();
    assert!(directory.path().is_absolute());
    assert_eq!(directory.path(), directory.path().canonicalize().unwrap());
}

#[tokio::test]
async fn file_rolls_local_hours_and_deduplicates_request_ids() {
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    let mut first = record("first", "owner");
    first.created_at = Local
        .with_ymd_and_hms(2026, 9, 7, 9, 15, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    let mut second = record("second", "owner");
    second.created_at = first.created_at + Duration::hours(1);
    store
        .append_batch(&[first.clone(), first.clone(), second.clone()])
        .await
        .unwrap();
    assert!(directory.path().join("logs/2026-09-07/09.log").is_file());
    assert!(directory.path().join("logs/2026-09-07/10.log").is_file());
    let page = store
        .query(LogQuery {
            from: Some(first.created_at - Duration::minutes(1)),
            to: Some(second.created_at + Duration::minutes(1)),
            ..LogQuery::for_user("owner")
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].request_id, "second");
    assert_eq!(
        page.items
            .iter()
            .filter(|row| row.request_id == "first")
            .count(),
        1
    );
}

#[tokio::test]
async fn file_enforces_owner_and_filters_before_pagination() {
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    let mut older = record("older", "owner");
    older.created_at -= Duration::seconds(5);
    store
        .append_batch(&[record("foreign", "other"), older, record("newer", "owner")])
        .await
        .unwrap();
    let page = store
        .query(LogQuery {
            page: 2,
            page_size: 1,
            model: Some("gpt-test".into()),
            ..LogQuery::for_user("owner")
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].request_id, "older");
    assert!(page.items.iter().all(|row| row.user_id == "owner"));
    assert!(store
        .query(LogQuery {
            user_id: Some("other".into()),
            ..LogQuery::for_user("owner")
        })
        .await
        .is_err());
}

#[tokio::test]
async fn file_recovers_partial_tail_without_exposing_bad_lines() {
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    let first = record("first", "owner");
    store.append_batch(&[first.clone()]).await.unwrap();
    let path = directory.path().join("logs").join(
        first
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d/%H.log")
            .to_string(),
    );
    let mut output = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    output.write_all(b"{\"token\":\"SECRET").unwrap();
    drop(output);
    store
        .append_batch(&[record("second", "owner")])
        .await
        .unwrap();
    let page = store.query(LogQuery::for_user("owner")).await.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.malformed_lines, 1);
    assert!(!serde_json::to_string(&page).unwrap().contains("SECRET"));
}

#[tokio::test]
async fn file_rejects_unsafe_root_and_unbounded_queries() {
    assert!(FileLogStore::open("relative/logs".into()).await.is_err());
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    assert!(store
        .query(LogQuery {
            from: Some(Utc::now() - Duration::days(32)),
            ..LogQuery::default()
        })
        .await
        .is_err());
    assert!(store
        .query(LogQuery {
            page_size: 101,
            ..LogQuery::default()
        })
        .await
        .is_err());
    assert!(store
        .query(LogQuery {
            page: 0,
            ..LogQuery::default()
        })
        .await
        .is_err());
    assert!(store
        .query(LogQuery {
            page: u32::MAX,
            ..LogQuery::default()
        })
        .await
        .is_err());
}

#[tokio::test]
async fn retention_deletes_only_expired_canonical_logs() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let store = FileLogStore::open(root.clone()).await.unwrap();
    let mut old = record("old", "owner");
    old.created_at -= Duration::days(40);
    store
        .append_batch(&[old.clone(), record("new", "owner")])
        .await
        .unwrap();
    let old_day = root.join(
        old.created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string(),
    );
    std::fs::write(old_day.join("keep.txt"), "keep").unwrap();
    std::fs::create_dir(root.join("not-a-date")).unwrap();
    std::fs::write(root.join("not-a-date/09.log"), "keep").unwrap();
    assert_eq!(store.retention(None).await.unwrap(), 0);
    assert_eq!(store.retention(Some(30)).await.unwrap(), 1);
    assert!(old_day.join("keep.txt").exists());
    assert!(root.join("not-a-date/09.log").exists());
}

#[tokio::test]
async fn records_redact_free_text_and_reject_negative_counters() {
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    let mut unsafe_record = record("redaction", "owner");
    unsafe_record.error_code = Some("Authorization: Bearer SECRET".into());
    unsafe_record.endpoint = "/v1/responses?token=SECRET".into();
    unsafe_record.model = "a prompt containing SECRET".into();
    store.append_batch(&[unsafe_record]).await.unwrap();
    let serialized =
        serde_json::to_string(&store.query(LogQuery::for_user("owner")).await.unwrap()).unwrap();
    assert!(!serialized.contains("SECRET"));
    assert!(serialized.contains("amountUsdMicros"));
    assert!(!serialized.contains("amount_usd_micros"));
    let mut invalid = record("negative", "owner");
    invalid.input_tokens = -1;
    assert!(store.append_batch(&[invalid]).await.is_err());
}

#[tokio::test]
async fn memory_queue_keeps_inflight_records_charged_until_ack() {
    let queue = MemoryLogQueue::new(1, 100_000);
    assert!(queue.try_enqueue(record("first", "owner")).await.unwrap());
    let batch = queue.receive(100).await.unwrap();
    assert_eq!(batch.records.len(), 1);
    assert_eq!(queue.stats().await.unwrap().pending, 1);
    assert!(!queue.try_enqueue(record("second", "owner")).await.unwrap());
    assert_eq!(
        queue.receive(100).await.unwrap().records[0].request_id,
        "first"
    );
    queue.ack(&batch).await.unwrap();
    queue.ack(&batch).await.unwrap();
    assert_eq!(queue.stats().await.unwrap().pending_bytes, 0);
    assert!(queue.try_enqueue(record("second", "owner")).await.unwrap());
}

#[tokio::test]
async fn memory_queue_enforces_byte_limit_independently_of_count() {
    let first = record("first", "owner");
    let size = serde_json::to_vec(&first.sanitized().unwrap())
        .unwrap()
        .len();
    let mut second = first.clone();
    second.request_id = "other".into();
    let queue = MemoryLogQueue::new(100, size);
    assert!(queue.try_enqueue(first).await.unwrap());
    assert!(!queue.try_enqueue(second).await.unwrap());
    assert_eq!(queue.stats().await.unwrap().pending_bytes, size);
    assert!(MemoryLogQueue::new(100, 1)
        .try_enqueue(record("large", "owner"))
        .await
        .is_err());
}

#[tokio::test]
async fn runtime_clones_share_worker_and_shutdown_drains_before_returning() {
    let directory = canonical_test_directory();
    let runtime = LogRuntime::default();
    assert!(!runtime.status().await.unwrap().configured);
    runtime
        .configure(LogConfig {
            directory: Some(directory.path().join("logs")),
            ..LogConfig::default()
        })
        .await
        .unwrap();
    runtime
        .clone()
        .enqueue(record("first", "owner"))
        .await
        .unwrap();
    runtime.shutdown().await.unwrap();
    let status = runtime.status().await.unwrap();
    assert_eq!(status.pending, 0);
    assert!(!status.accepting);
    assert!(status.last_success_at.is_some());
    assert_eq!(
        runtime
            .query(LogQuery::for_user("owner"))
            .await
            .unwrap()
            .total,
        1
    );
    assert!(runtime.enqueue(record("late", "owner")).await.is_err());
}

#[tokio::test]
async fn runtime_switch_drains_old_store_and_preserves_old_on_invalid_config() {
    let directory = canonical_test_directory();
    let runtime = LogRuntime::default();
    let old_root = directory.path().join("old");
    runtime
        .configure(LogConfig {
            directory: Some(old_root.clone()),
            ..LogConfig::default()
        })
        .await
        .unwrap();
    runtime.enqueue(record("old", "owner")).await.unwrap();
    assert!(runtime
        .configure(LogConfig {
            directory: Some("relative".into()),
            ..LogConfig::default()
        })
        .await
        .is_err());
    runtime.enqueue(record("still-old", "owner")).await.unwrap();
    runtime
        .configure(LogConfig {
            directory: Some(directory.path().join("new")),
            ..LogConfig::default()
        })
        .await
        .unwrap();
    assert_eq!(
        FileLogStore::open(old_root)
            .await
            .unwrap()
            .query(LogQuery::for_user("owner"))
            .await
            .unwrap()
            .total,
        2
    );
    runtime.enqueue(record("new", "owner")).await.unwrap();
    runtime.shutdown().await.unwrap();
    assert_eq!(
        runtime
            .query(LogQuery::for_user("owner"))
            .await
            .unwrap()
            .total,
        1
    );
}

#[tokio::test]
async fn runtime_reports_backpressure_retries_and_timed_out_shutdown_without_dropping() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let runtime = LogRuntime::default();
    runtime
        .configure(LogConfig {
            directory: Some(root.clone()),
            max_records: 1,
            batch_size: 1,
            enqueue_timeout_ms: 20,
            shutdown_timeout_ms: 50,
            ..LogConfig::default()
        })
        .await
        .unwrap();
    let first = record("retry", "owner");
    let day = root.join(
        first
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string(),
    );
    std::fs::write(&day, "blocks the date directory").unwrap();
    runtime.enqueue(first).await.unwrap();
    assert!(runtime.enqueue(record("full", "owner")).await.is_err());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while runtime.status().await.unwrap().retries == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("log worker reports the blocked write before testing shutdown");
    assert!(runtime.shutdown().await.is_err());
    let status = runtime.status().await.unwrap();
    assert_eq!(status.pending, 1);
    assert_eq!(status.shutdown_pending, 1);
    assert!(status.retries > 0);
    std::fs::remove_file(day).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if runtime.status().await.unwrap().pending == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await.unwrap();
    assert_eq!(
        runtime
            .query(LogQuery::for_user("owner"))
            .await
            .unwrap()
            .total,
        1
    );
}

#[test]
fn config_never_serializes_credentials_and_rejects_unsafe_limits() {
    let mut config = LogConfig {
        redis_url: Some("redis://:SECRET@localhost/0".into()),
        postgres_url: Some("postgres://name:SECRET@localhost/db".into()),
        ..LogConfig::default()
    };
    assert!(!serde_json::to_string(&config).unwrap().contains("SECRET"));
    assert!(!format!("{config:?}").contains("SECRET"));
    config.max_records = 0;
    assert!(config.validate().is_err());
    assert!(LogConfig {
        queue: LogQueueKind::Redis,
        ..LogConfig::default()
    }
    .validate()
    .is_err());
}

#[tokio::test]
async fn reading_historical_logs_preserves_the_recorded_timezone_offset() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let store = FileLogStore::open(root.clone()).await.unwrap();
    let mut historical = record("historical-offset", "owner");
    historical.utc_offset_minutes = -240;
    let path = root.join(
        historical
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d/%H.log")
            .to_string(),
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string(&historical).unwrap()),
    )
    .unwrap();
    let result = store.query(LogQuery::for_user("owner")).await.unwrap();
    assert_eq!(result.items[0].utc_offset_minutes, -240);
}

#[tokio::test]
async fn runtime_advertises_no_admission_when_queue_is_full() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let runtime = LogRuntime::default();
    runtime
        .configure(LogConfig {
            directory: Some(root.clone()),
            max_records: 1,
            batch_size: 1,
            ..LogConfig::default()
        })
        .await
        .unwrap();
    let first = record("capacity-status", "owner");
    let blocker = root.join(
        first
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string(),
    );
    std::fs::write(&blocker, "blocked").unwrap();
    runtime.enqueue(first).await.unwrap();
    let full = runtime.status().await.unwrap();
    std::fs::remove_file(blocker).unwrap();
    runtime.shutdown().await.unwrap();
    assert_eq!(full.pending, 1);
    assert!(!full.accepting);
}

#[tokio::test]
async fn file_rejects_batches_larger_than_the_consumer_limit() {
    let directory = canonical_test_directory();
    let store = FileLogStore::open(directory.path().join("logs"))
        .await
        .unwrap();
    assert!(store
        .append_batch(&vec![record("oversize-batch", "owner"); 501])
        .await
        .is_err());
}

#[tokio::test]
async fn oversized_and_incomplete_jsonl_lines_are_counted_without_loading_or_disclosing_them() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let store = FileLogStore::open(root.clone()).await.unwrap();
    let valid = record("bounded-reader", "owner");
    let path = root.join(
        valid
            .created_at
            .with_timezone(&Local)
            .format("%Y-%m-%d/%H.log")
            .to_string(),
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = format!(
        "{}\n{}\n{{\"partialSecret\":\"SECRET",
        "X".repeat(100_000),
        serde_json::to_string(&valid).unwrap()
    );
    std::fs::write(path, content).unwrap();
    let page = store.query(LogQuery::for_user("owner")).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.malformed_lines, 2);
    assert!(!serde_json::to_string(&page).unwrap().contains("SECRET"));
}

#[tokio::test]
async fn file_query_and_retention_never_follow_linked_date_directories() {
    let directory = canonical_test_directory();
    let root = directory.path().join("logs");
    let outside = directory.path().join("outside");
    let store = FileLogStore::open(root.clone()).await.unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("09.log"), "outside-secret").unwrap();
    let old_date = (Local::now().date_naive() - Duration::days(40))
        .format("%Y-%m-%d")
        .to_string();
    let link = root.join(&old_date);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(output.status.success(), "test junction creation failed");
    }
    let query = LogQuery {
        from: Some(Utc::now() - Duration::days(41)),
        to: Some(Utc::now() - Duration::days(39)),
        ..LogQuery::for_user("owner")
    };
    assert!(store.query(query).await.is_err());
    assert_eq!(store.retention(Some(30)).await.unwrap(), 0);
    assert_eq!(
        std::fs::read_to_string(outside.join("09.log")).unwrap(),
        "outside-secret"
    );
    assert!(FileLogStore::open(link.clone()).await.is_err());
    assert!(FileLogStore::open(link.join("nested-logs")).await.is_err());
    assert!(!outside.join("nested-logs").exists());
}

#[test]
fn runtime_public_futures_are_send_for_http_handlers() {
    fn require_send<Value: Send>(_: Value) {}
    let runtime = LogRuntime::default();
    require_send(runtime.configure(LogConfig::default()));
    require_send(runtime.enqueue(record("send", "owner")));
    require_send(runtime.query(LogQuery::default()));
    require_send(runtime.status());
    require_send(runtime.shutdown());
}
