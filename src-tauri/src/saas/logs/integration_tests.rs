use super::tests::record;
use super::*;

fn external_config() -> LogConfig {
    LogConfig {
        redis_url: std::env::var("SAAS_LOG_REDIS_URL").ok(),
        postgres_url: std::env::var("SAAS_LOG_POSTGRES_URL").ok(),
        instance_id: format!("test-{}", uuid::Uuid::new_v4()),
        ..LogConfig::default()
    }
}

#[tokio::test]
#[ignore = "requires a real Redis service: SAAS_LOG_REDIS_URL"]
async fn redis_recovers_unacknowledged_entries_and_isolates_instances() {
    let mut config = external_config();
    config.queue = LogQueueKind::Redis;
    assert!(config.redis_url.is_some(), "set SAAS_LOG_REDIS_URL");
    config.max_records = 1;
    config.batch_size = 1;
    let queue = RedisLogQueue::connect(&config).await.unwrap();
    assert!(queue
        .try_enqueue(record("redis-pending", "owner"))
        .await
        .unwrap());
    let first = queue.receive(10).await.unwrap();
    assert_eq!(first.records.len(), 1);
    assert_eq!(queue.stats().await.unwrap().pending, 1);
    assert!(!queue
        .try_enqueue(record("redis-full", "owner"))
        .await
        .unwrap());
    drop(queue);
    let queue = RedisLogQueue::connect(&config).await.unwrap();
    let replayed = queue.receive(10).await.unwrap();
    assert_eq!(replayed.receipts, first.receipts);
    assert_eq!(replayed.records[0].request_id, "redis-pending");
    let mut foreign_config = config.clone();
    foreign_config.instance_id.push_str("-foreign");
    let foreign = RedisLogQueue::connect(&foreign_config).await.unwrap();
    assert!(foreign.receive(10).await.unwrap().records.is_empty());
    queue.ack(&replayed).await.unwrap();
    queue.ack(&replayed).await.unwrap();
    assert_eq!(queue.stats().await.unwrap().pending, 0);
    assert_eq!(queue.stats().await.unwrap().pending_bytes, 0);
    super::redis::cleanup_test_queue(&queue).await;
    super::redis::cleanup_test_queue(&foreign).await;
}

#[tokio::test]
#[ignore = "requires a real PostgreSQL service: SAAS_LOG_POSTGRES_URL"]
async fn postgres_commits_idempotently_and_filters_owners_before_paging() {
    let config = external_config();
    assert!(config.postgres_url.is_some(), "set SAAS_LOG_POSTGRES_URL");
    let store = PostgresLogStore::connect(&config).await.unwrap();
    let owner = format!("owner-{}", uuid::Uuid::new_v4());
    let mut first = record(&format!("pg-{}", uuid::Uuid::new_v4()), &owner);
    first.created_at -= chrono::Duration::seconds(10);
    let second = record(&format!("pg-{}", uuid::Uuid::new_v4()), &owner);
    let other = record(&format!("pg-{}", uuid::Uuid::new_v4()), "other");
    store
        .append_batch(&[first.clone(), second.clone(), other.clone()])
        .await
        .unwrap();
    store.append_batch(&[first.clone()]).await.unwrap();
    let page = store
        .query(LogQuery {
            page: 2,
            page_size: 1,
            ..LogQuery::for_user(&owner)
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].request_id, first.request_id);
    assert!(store
        .query(LogQuery {
            user_id: Some("other".into()),
            ..LogQuery::for_user(&owner)
        })
        .await
        .is_err());
    super::postgres::cleanup_test_records(
        &store,
        &[first.request_id, second.request_id, other.request_id],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database: SAAS_LOG_POSTGRES_URL; deletes expired logs"]
async fn postgres_retention_removes_expired_records_only_when_enabled() {
    let config = external_config();
    assert!(
        config.postgres_url.is_some(),
        "set SAAS_LOG_POSTGRES_URL to a disposable test database"
    );
    let store = PostgresLogStore::connect(&config).await.unwrap();
    let owner = format!("retention-{}", uuid::Uuid::new_v4());
    let mut old = record(&format!("old-{}", uuid::Uuid::new_v4()), &owner);
    old.created_at -= chrono::Duration::days(40);
    let recent = record(&format!("new-{}", uuid::Uuid::new_v4()), &owner);
    store
        .append_batch(&[old.clone(), recent.clone()])
        .await
        .unwrap();
    let old_query = LogQuery {
        from: Some(old.created_at - chrono::Duration::hours(1)),
        to: Some(old.created_at + chrono::Duration::hours(1)),
        ..LogQuery::for_user(&owner)
    };
    assert_eq!(store.retention(None).await.unwrap(), 0);
    assert_eq!(store.query(old_query.clone()).await.unwrap().total, 1);
    assert!(store.retention(Some(30)).await.unwrap() >= 1);
    assert_eq!(store.query(old_query).await.unwrap().total, 0);
    assert_eq!(
        store.query(LogQuery::for_user(&owner)).await.unwrap().total,
        1
    );
    super::postgres::cleanup_test_records(&store, &[old.request_id, recent.request_id]).await;
}

#[tokio::test]
#[ignore = "requires real Redis and PostgreSQL: SAAS_LOG_REDIS_URL, SAAS_LOG_POSTGRES_URL"]
async fn all_four_queue_store_combinations_drain_and_query() {
    let directory = tempfile::tempdir().unwrap();
    for queue in [LogQueueKind::Memory, LogQueueKind::Redis] {
        for store in [LogStoreKind::File, LogStoreKind::Postgres] {
            let mut config = external_config();
            assert!(
                config.redis_url.is_some() && config.postgres_url.is_some(),
                "set both external service URLs"
            );
            config.queue = queue;
            config.store = store;
            config.directory = Some(directory.path().join(&config.instance_id));
            let runtime = LogRuntime::default();
            runtime.configure(config.clone()).await.unwrap();
            let request_id = format!("combination-{}", uuid::Uuid::new_v4());
            let owner = format!("owner-{}", uuid::Uuid::new_v4());
            runtime.enqueue(record(&request_id, &owner)).await.unwrap();
            runtime.shutdown().await.unwrap();
            let result = runtime.query(LogQuery::for_user(&owner)).await.unwrap();
            assert_eq!(result.total, 1);
            assert_eq!(result.items[0].request_id, request_id);
            if queue == LogQueueKind::Redis {
                super::redis::cleanup_test_queue(&RedisLogQueue::connect(&config).await.unwrap())
                    .await;
            }
            if store == LogStoreKind::Postgres {
                super::postgres::cleanup_test_records(
                    &PostgresLogStore::connect(&config).await.unwrap(),
                    &[request_id],
                )
                .await;
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires permission-denied service URLs: SAAS_LOG_REDIS_DENIED_URL, SAAS_LOG_POSTGRES_DENIED_URL"]
async fn external_permission_denials_never_activate_a_fallback() {
    let config = LogConfig {
        redis_url: Some(
            std::env::var("SAAS_LOG_REDIS_DENIED_URL").expect("set SAAS_LOG_REDIS_DENIED_URL"),
        ),
        postgres_url: Some(
            std::env::var("SAAS_LOG_POSTGRES_DENIED_URL")
                .expect("set SAAS_LOG_POSTGRES_DENIED_URL"),
        ),
        ..external_config()
    };
    assert!(RedisLogQueue::connect(&config).await.is_err());
    assert!(PostgresLogStore::connect(&config).await.is_err());
}
