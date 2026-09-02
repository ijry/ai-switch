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

#[tokio::test]
async fn migrations_create_route_credential_models_table() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");

    let columns = sqlx::query("PRAGMA table_info(route_credential_models)")
        .fetch_all(&pool)
        .await
        .expect("model cooldown columns");
    let names: Vec<String> = columns
        .iter()
        .map(|column| column.get::<String, _>("name"))
        .collect();
    for expected in [
        "route_credential_id",
        "model_key",
        "status",
        "transient_failure_count",
        "cooldown_until",
        "semantic_failure_streak_count",
        "semantic_failure_streak_fingerprint",
        "last_failure_kind",
        "last_failure_message",
        "last_failure_response_json",
        "created_at",
        "updated_at",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }

    // status is constrained to the three model-level states.
    sqlx::query(
        "INSERT INTO route_credentials
         (id, platform, kind, display_name, status, sort_order, secret_payload_json,
          config_json, preview_json, created_at, updated_at)
         VALUES ('cred-1', 'codex', 'api', 'Fixture', 'ok', 0, '{}', '{}', '{}', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("seed credential");
    let rejected = sqlx::query(
        "INSERT INTO route_credential_models
         (route_credential_id, model_key, status, created_at, updated_at)
         VALUES ('cred-1', 'gpt-5.6-sol', 'revoked', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await;
    assert!(
        rejected.is_err(),
        "status must reject values outside the three model states"
    );

    sqlx::query(
        "INSERT INTO route_credential_models
         (route_credential_id, model_key, status, created_at, updated_at)
         VALUES ('cred-1', 'gpt-5.6-sol', 'paused', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("insert paused model");
    sqlx::query("DELETE FROM route_credentials WHERE id = 'cred-1'")
        .execute(&pool)
        .await
        .expect("delete credential");
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM route_credential_models")
        .fetch_one(&pool)
        .await
        .expect("count after cascade");
    assert_eq!(
        remaining, 0,
        "deleting an account must cascade its model rows"
    );
}
