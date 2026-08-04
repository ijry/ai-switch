use super::{create_memory_pool, run_migrations};
use crate::database::repositories::target_repository::TargetRepository;
use sqlx::Row;

#[tokio::test]
async fn migrations_create_foundation_tables() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");

    let row = sqlx::query("SELECT COUNT(*) as count FROM sqlite_master WHERE type = 'table' AND name IN ('target_apps', 'providers', 'official_accounts', 'batches', 'batch_items', 'import_jobs')")
        .fetch_one(&pool)
        .await
        .expect("table count");

    let count: i64 = row.get("count");
    assert_eq!(count, 6);

    let columns = sqlx::query("PRAGMA table_info(target_apps)")
        .fetch_all(&pool)
        .await
        .expect("target app columns");
    assert!(columns
        .iter()
        .any(|column| column.get::<String, _>("name") == "platform"));

    TargetRepository::ensure_defaults(&pool)
        .await
        .expect("target defaults");
    let mappings = sqlx::query("SELECT key, platform FROM target_apps ORDER BY key")
        .fetch_all(&pool)
        .await
        .expect("target platform mappings");
    let mapping = |key: &str| {
        mappings
            .iter()
            .find(|row| row.get::<String, _>("key") == key)
            .and_then(|row| row.get::<Option<String>, _>("platform"))
    };
    assert_eq!(mapping("claude_code").as_deref(), Some("claude"));
    assert_eq!(mapping("grok").as_deref(), Some("grok"));
    assert_eq!(mapping("hermes").as_deref(), Some("hermes"));
}
