use crate::error::AppError;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub const MAX_MONEY: i64 = 9_000_000_000_000_000;

pub fn invalid(code: &'static str, message: impl Into<String>) -> AppError {
    AppError::Validation {
        code,
        message: message.into(),
        details: None,
        recoverable: true,
    }
}

pub fn db_error(_: sqlx::Error) -> AppError {
    AppError::Database {
        code: "saas.database",
        message: "SaaS database operation failed".into(),
        details: None,
        recoverable: true,
    }
}

pub fn hash_secret(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub fn random_secret(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub fn now() -> i64 {
    Utc::now().timestamp()
}

pub fn timestamp(value: i64) -> String {
    DateTime::<Utc>::from_timestamp(value, 0)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_expiry(value: Option<&Value>) -> Result<Option<i64>, AppError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let parsed = DateTime::parse_from_rfc3339(text)
                .map_err(|_| invalid("saas.validation", "expiresAt must be an RFC3339 timestamp"))?
                .timestamp();
            if parsed <= now() {
                return Err(invalid("saas.validation", "Expiry must be in the future"));
            }
            Ok(Some(parsed))
        }
        _ => Err(invalid(
            "saas.validation",
            "expiresAt must be an RFC3339 timestamp",
        )),
    }
}

pub fn text<'a>(payload: &'a Value, key: &str) -> Result<&'a str, AppError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 1024)
        .ok_or_else(|| invalid("saas.validation", format!("{key} is required")))
}

pub fn money(value: i128) -> Result<i64, AppError> {
    if value.unsigned_abs() > MAX_MONEY as u128 {
        return Err(invalid(
            "saas.amount_overflow",
            "Amount exceeds the supported range",
        ));
    }
    Ok(value as i64)
}

pub fn page(payload: &Value) -> Result<(i64, i64), AppError> {
    let page = payload
        .get("page")
        .map(|value| {
            value
                .as_i64()
                .ok_or_else(|| invalid("saas.validation", "page must be an integer"))
        })
        .transpose()?
        .unwrap_or(1);
    let size = payload
        .get("pageSize")
        .map(|value| {
            value
                .as_i64()
                .ok_or_else(|| invalid("saas.validation", "pageSize must be an integer"))
        })
        .transpose()?
        .unwrap_or(50);
    if !(1..=1_000_000).contains(&page) || !(1..=200).contains(&size) {
        return Err(invalid("saas.validation", "Invalid pagination"));
    }
    Ok((size, (page - 1) * size))
}

pub async fn begin(pool: &SqlitePool) -> Result<Transaction<'_, Sqlite>, AppError> {
    pool.begin_with("BEGIN IMMEDIATE").await.map_err(db_error)
}

pub async fn audit(
    connection: &mut SqliteConnection,
    operation: &str,
    target: Option<&str>,
    details: Value,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO saas_admin_audit(id,actor,operation,target_id,details_json,created_at) VALUES(?,'administrator',?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(operation).bind(target).bind(details.to_string()).bind(now())
        .execute(connection).await.map_err(db_error)?;
    Ok(())
}

pub async fn require_user(
    connection: &mut SqliteConnection,
    user_id: &str,
) -> Result<(), AppError> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM saas_users WHERE id=?")
        .bind(user_id)
        .fetch_optional(connection)
        .await
        .map_err(db_error)?;
    if status.as_deref() != Some("active") {
        return Err(invalid("saas.unauthorized", "User is unavailable"));
    }
    Ok(())
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), AppError> {
    let mut transaction = begin(pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS saas_schema_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL,applied_at INTEGER NOT NULL)")
        .execute(&mut *transaction).await.map_err(db_error)?;
    let migrations = [
        (1_i64, include_str!("../migrations/0001_saas.sql")),
        (2_i64, include_str!("../migrations/0002_password_login.sql")),
        (3_i64, include_str!("../migrations/0003_growth.sql")),
        (
            4_i64,
            include_str!("../migrations/0004_daily_subscription.sql"),
        ),
    ];
    let applied: Vec<(i64, String)> =
        sqlx::query_as("SELECT version,checksum FROM saas_schema_migrations ORDER BY version")
            .fetch_all(&mut *transaction)
            .await
            .map_err(db_error)?;
    if applied.iter().any(|(version, stored)| {
        migrations
            .iter()
            .find(|(known, _)| known == version)
            .map(|(_, sql)| stored != &hash_secret(&sql.replace("\r\n", "\n")))
            .unwrap_or(true)
    }) {
        return Err(invalid(
            "saas.migration_checksum",
            "SaaS schema version or checksum mismatch",
        ));
    }
    for (version, source) in migrations {
        if applied.iter().any(|(current, _)| *current == version) {
            continue;
        }
        let sql = source.replace("\r\n", "\n");
        let checksum = hash_secret(&sql);
        sqlx::Executor::execute(&mut *transaction, sql.as_str())
            .await
            .map_err(db_error)?;
        sqlx::query(
            "INSERT INTO saas_schema_migrations(version,checksum,applied_at) VALUES(?,?,?)",
        )
        .bind(version)
        .bind(checksum)
        .bind(now())
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        if version == 1 {
            sqlx::query(
                "INSERT INTO saas_settings(key,value_json,updated_at) VALUES('instance_id',?,?)",
            )
            .bind(serde_json::json!(uuid::Uuid::new_v4().to_string()).to_string())
            .bind(now())
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
    }
    transaction.commit().await.map_err(db_error)
}

#[cfg(test)]
pub(crate) async fn test_pool() -> SqlitePool {
    let pool = crate::database::create_memory_pool().await.unwrap();
    migrate(&pool).await.unwrap();
    pool
}

#[cfg(test)]
pub(crate) async fn test_enable(pool: &SqlitePool) {
    let value = serde_json::json!({"enabled":true,"registrationEnabled":true,"passwordLoginEnabled":true,"publicBaseUrl":"https://saas.example","githubClientId":"test-client","githubClientSecretConfigured":true,"exchangeRateMicros":7_000_000});
    sqlx::query("INSERT INTO saas_settings(key,value_json,updated_at) VALUES('config',?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
        .bind(value.to_string()).bind(now()).execute(pool).await.unwrap();
}

#[cfg(test)]
pub(crate) async fn test_host(pool: &SqlitePool) {
    crate::database::run_migrations(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO route_pool_groups(id,platform,name,sort_order,is_internal,is_active,created_at,updated_at) VALUES('saas-test-group','codex','SaaS test group',10,0,0,'2026-01-01','2026-01-01')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO batches(id,name,source,created_at,updated_at) VALUES('batch-custom','Custom','manual','2026-01-01','2026-01-01')").execute(pool).await.unwrap();
    for (identifier, batch, platform, status, archived, enabled) in [
        ("account-default", None, "codex", "ok", None, 1),
        (
            "account-custom",
            Some("batch-custom"),
            "codex",
            "ok",
            None,
            1,
        ),
        ("account-foreign", None, "claude", "ok", None, 1),
        ("account-disabled", None, "codex", "paused", None, 1),
        (
            "account-archived",
            None,
            "codex",
            "ok",
            Some("2026-01-01"),
            1,
        ),
        ("account-not-pooled", None, "codex", "ok", None, 0),
    ] {
        sqlx::query("INSERT OR IGNORE INTO route_credentials(id,platform,kind,display_name,status,batch_id,config_json,archived_at,created_at,updated_at) VALUES(?,?,'api',?,?,?, ?,?,'2026-01-01','2026-01-01')")
            .bind(identifier).bind(platform).bind(identifier).bind(status).bind(batch)
            .bind(r#"{"model_mappings":[{"from":"gpt-test","to":"gpt-test"}]}"#).bind(archived).execute(pool).await.unwrap();
        let group_id = match identifier.as_ref() {
            "account-archived" => "codex-archived",
            "account-not-pooled" => "codex-out",
            "account-foreign" => "claude-default",
            _ => "saas-test-group",
        };
        sqlx::query("INSERT OR IGNORE INTO route_pool_members(id,platform,route_credential_id,enabled,group_id,created_at,updated_at) VALUES(?,?,?,?,?,'2026-01-01','2026-01-01')")
            .bind(identifier).bind(platform).bind(identifier).bind(enabled).bind(group_id).execute(pool).await.unwrap();
    }
}

#[cfg(test)]
pub(crate) fn test_group_payload() -> Value {
    serde_json::json!({"id":"saas-test-group","multiplierMicros":1_000_000,"maxOutputTokens":4096,"timeoutSeconds":120,"maxConcurrency":10,"models":[{"model":"gpt-test","inputPriceMicros":1_000_000,"cachePriceMicros":100_000,"outputPriceMicros":2_000_000}]})
}

#[cfg(test)]
pub(crate) async fn test_user_group(pool: &SqlitePool) -> (String, String) {
    test_enable(pool).await;
    test_host(pool).await;
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO saas_users(id,github_id,login,github_created_at,created_at,updated_at) VALUES(?,?,'tester','2020-01-01T00:00:00Z',?,?)")
        .bind(&user_id).bind(&user_id).bind(now()).bind(now()).execute(pool).await.unwrap();
    let group = crate::saas::domain::admin(pool, "groups.save", test_group_payload())
        .await
        .unwrap();
    (user_id, group["id"].as_str().unwrap().into())
}

#[cfg(test)]
pub(crate) async fn test_principal(
    pool: &SqlitePool,
    balance: i64,
) -> crate::saas::billing::ApiPrincipal {
    let (user_id, group_id) = test_user_group(pool).await;
    sqlx::query("UPDATE saas_users SET balance_micros=? WHERE id=?")
        .bind(balance)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();
    let key = crate::saas::domain::user(
        pool,
        &user_id,
        "keys.create",
        serde_json::json!({"groupId":group_id,"name":"test"}),
    )
    .await
    .unwrap();
    crate::saas::billing::authenticate_key(pool, key["plaintextKey"].as_str().unwrap())
        .await
        .unwrap()
}

#[cfg(test)]
#[tokio::test]
async fn service_futures_are_send_for_http_handlers() {
    fn assert_send<Future: Send>(_: Future) {}
    let pool = test_pool().await;
    assert_send(migrate(&pool));
    assert_send(crate::saas::config::load(&pool));
    assert_send(crate::saas::config::save(&pool, serde_json::json!({})));
    assert_send(crate::saas::auth::start_oauth(&pool, None));
    assert_send(crate::saas::domain::admin(
        &pool,
        "overview",
        serde_json::json!({}),
    ));
}
