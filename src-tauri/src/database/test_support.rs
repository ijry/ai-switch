use super::{create_memory_pool, run_migrations};
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
}
