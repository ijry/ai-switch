use super::*;
use crate::saas::repository;
use crate::security::SecretStore;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub(crate) struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<String, AppError> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| repository::invalid("saas.secret", "Secret is not configured"))
    }
}

#[tokio::test]
async fn isolated_migration_is_idempotent_and_detects_checksum_changes() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    repository::migrate(&pool).await.unwrap();
    repository::migrate(&pool).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(tables
        .iter()
        .all(|name| name.starts_with("saas_") || name.starts_with("sqlite_")));
    assert_eq!(
        tables
            .iter()
            .filter(|name| name.as_str() == "saas_schema_migrations")
            .count(),
        1
    );
    assert!(!load(&pool).await.unwrap().enabled);
    sqlx::query("UPDATE saas_schema_migrations SET checksum='changed'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(repository::migrate(&pool).await.is_err());
}

#[tokio::test]
async fn daily_subscription_migration_upgrades_an_existing_growth_schema() {
    let pool = crate::database::create_memory_pool().await.unwrap();
    sqlx::query("CREATE TABLE saas_schema_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL,applied_at INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    for (version, source) in [
        (1_i64, include_str!("../migrations/0001_saas.sql")),
        (2_i64, include_str!("../migrations/0002_password_login.sql")),
        (3_i64, include_str!("../migrations/0003_growth.sql")),
    ] {
        let sql = source.replace("\r\n", "\n");
        sqlx::Executor::execute(&pool, sql.as_str()).await.unwrap();
        sqlx::query(
            "INSERT INTO saas_schema_migrations(version,checksum,applied_at) VALUES(?,?,?)",
        )
        .bind(version)
        .bind(repository::hash_secret(&sql))
        .bind(repository::now())
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO saas_redeem_codes(id,batch_id,token_hash,prefix,suffix,amount_micros,status,created_at) VALUES('preserve','batch','token-hash','prefix','suff',5,'active',0)")
        .execute(&pool)
        .await
        .unwrap();
    repository::migrate(&pool).await.unwrap();
    repository::migrate(&pool).await.unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM saas_schema_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('saas_oauth_states')")
            .fetch_all(&pool)
            .await
            .unwrap();
    let table_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='saas_subscription_daily_usage'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let codes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM saas_redeem_codes WHERE id='preserve'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 5);
    assert!(columns.iter().any(|column| column == "invite_code"));
    assert!(!columns.iter().any(|column| column == "invite_code_hash"));
    assert_eq!(table_exists, 1);
    assert_eq!(codes, 1);
}

#[tokio::test]
async fn optional_exchange_rate_is_validated_and_secrets_are_never_persisted_in_sqlite() {
    let pool = repository::test_pool().await;
    let secrets = MemorySecrets::default();
    unlock(&pool, "ai-switch-ok").await.unwrap();
    let payload = serde_json::json!({"enabled":true,"publicBaseUrl":"https://saas.example", "githubClientId":"client-id", "githubClientSecret":"client-secret-value"});
    let mut valid = payload;
    valid["exchangeRateMicros"] = serde_json::json!(-1);
    assert!(save_with_secret_store(&pool, valid.clone(), &secrets)
        .await
        .is_err());
    valid["exchangeRateMicros"] = serde_json::json!(7_000_000);
    let saved = save_with_secret_store(&pool, valid, &secrets)
        .await
        .unwrap();
    assert!(saved.enabled);
    assert!(saved.github_client_secret_configured);
    assert!(!serde_json::to_string(&saved)
        .unwrap()
        .contains("client-secret-value"));
    let persisted: String =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key='config'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!persisted.contains("client-secret-value"));
    save_with_secret_store(&pool, serde_json::json!({"siteName":"Renamed"}), &secrets)
        .await
        .unwrap();
    assert_eq!(
        github_secret_with_store(&pool, &secrets).await.unwrap(),
        "client-secret-value"
    );
    let public = public_config(&pool).await.unwrap();
    assert_eq!(public["exchangeRateMicros"], 7_000_000);
    assert_eq!(public["passwordLoginEnabled"], true);
    assert!(public.get("githubClientId").is_none());
    assert!(public.get("logs").is_none());
}

#[tokio::test]
async fn log_environment_references_survive_save_without_exposing_connection_secrets() {
    let pool = repository::test_pool().await;
    let saved = save(&pool, serde_json::json!({"logs":{"queue":"redis","store":"postgres","redisUrlEnv":"SAAS_REDIS","postgresUrlEnv":"SAAS_POSTGRES"}})).await.unwrap();
    assert_eq!(saved.logs["redisUrlEnv"], "SAAS_REDIS");
    assert!(save(
        &pool,
        serde_json::json!({"logs":{"redisUrl":"redis://user:secret@localhost:6379"}})
    )
    .await
    .is_err());
    assert_eq!(
        load(&pool).await.unwrap().logs["postgresUrlEnv"],
        "SAAS_POSTGRES"
    );
}

#[test]
fn public_origin_requires_https_except_explicit_loopback() {
    for bad in [
        "http://example.com",
        "https://example.com/path",
        "https://user:pass@example.com",
        "https://example.com?x=1",
        "http://127.0.0.1.evil.example",
    ] {
        assert!(validate_public_base_url(bad).is_err(), "{bad}");
    }
    for good in [
        "https://example.com",
        "http://127.0.0.1:1420",
        "http://[::1]:1420",
        "http://localhost:1420",
    ] {
        assert!(validate_public_base_url(good).is_ok(), "{good}");
    }
}

#[tokio::test]
async fn beta_unlock_is_persisted_without_enabling_or_storing_the_code() {
    let pool = repository::test_pool().await;
    assert!(!is_unlocked(&pool).await.unwrap());
    assert_eq!(
        unlock(&pool, "wrong-code").await.unwrap_err().code(),
        "saas.activation_code_invalid"
    );
    assert!(unlock(&pool, "ai-switch-nb").await.unwrap());
    assert!(is_unlocked(&pool).await.unwrap());
    assert!(!load(&pool).await.unwrap().enabled);
    let persisted: String =
        sqlx::query_scalar("SELECT value_json FROM saas_settings WHERE key='activation_unlocked'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted, "true");
    assert!(!persisted.contains("ai-switch-nb"));
}

#[tokio::test]
async fn enabling_requires_unlock_but_not_the_code_again() {
    let pool = repository::test_pool().await;
    let secrets = MemorySecrets::default();
    let payload = serde_json::json!({
        "enabled": true,
        "publicBaseUrl": "https://saas.example",
        "githubClientId": "client-id",
        "githubClientSecret": "client-secret-value",
        "exchangeRateMicros": 7_000_000
    });
    assert_eq!(
        save_with_secret_store(&pool, payload.clone(), &secrets)
            .await
            .unwrap_err()
            .code(),
        "saas.activation_required"
    );
    unlock(&pool, "ai-switch-ok").await.unwrap();
    assert!(
        save_with_secret_store(&pool, payload, &secrets)
            .await
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn enabled_preview_does_not_require_auth_or_billing_configuration() {
    let pool = repository::test_pool().await;
    let secrets = MemorySecrets::default();
    unlock(&pool, "ai-switch-ok").await.unwrap();
    let saved = save_with_secret_store(
        &pool,
        serde_json::json!({
            "enabled": true
        }),
        &secrets,
    )
    .await
    .unwrap();
    assert!(saved.enabled);
    assert!(saved.public_base_url.is_empty());
    assert!(saved.github_client_id.is_empty());
    assert!(saved.exchange_rate_micros.is_none());
}
