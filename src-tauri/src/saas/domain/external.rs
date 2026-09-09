use crate::error::AppError;
use crate::saas::repository::{self, db_error, invalid};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub async fn key_status(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT external_api_key_prefix FROM saas_users WHERE id=?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(db_error)?
            .flatten();
    Ok(json!({"configured":row.is_some(),"prefix":row}))
}
pub async fn rotate_key(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let plaintext = repository::random_secret("sk-saas-account-");
    let prefix = plaintext[..24].to_string();
    let changed = sqlx::query("UPDATE saas_users SET external_api_key_hash=?,external_api_key_prefix=?,updated_at=? WHERE id=?").bind(repository::hash_secret(&plaintext)).bind(&prefix).bind(repository::now()).bind(user_id).execute(pool).await.map_err(db_error)?.rows_affected();
    if changed == 0 {
        return Err(invalid("saas.user_not_found", "User does not exist"));
    }
    Ok(json!({"configured":true,"prefix":prefix,"plaintextKey":plaintext}))
}
pub async fn revoke_key(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    sqlx::query("UPDATE saas_users SET external_api_key_hash=NULL,external_api_key_prefix=NULL,updated_at=? WHERE id=?").bind(repository::now()).bind(user_id).execute(pool).await.map_err(db_error)?;
    Ok(json!({"configured":false,"prefix":null}))
}
pub async fn authenticate(pool: &SqlitePool, plaintext: &str) -> Result<String, AppError> {
    if !plaintext.starts_with("sk-saas-account-") || plaintext.len() > 256 {
        return Err(invalid("saas.unauthorized", "Invalid account security key"));
    }
    sqlx::query_scalar(
        "SELECT id FROM saas_users WHERE external_api_key_hash=? AND status='active'",
    )
    .bind(repository::hash_secret(plaintext))
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| invalid("saas.unauthorized", "Invalid account security key"))
}
type SubscriptionRow = (
    String,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    String,
    String,
);
async fn active_rows(
    pool: &SqlitePool,
    user_id: &str,
    all: bool,
) -> Result<Vec<SubscriptionRow>, AppError> {
    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    sqlx::query_as("SELECT subscriptions.id,subscriptions.plan_id,subscriptions.plan_name,subscriptions.quota_micros,COALESCE(daily.used_micros,0),COALESCE(daily.frozen_micros,0),subscriptions.starts_at,subscriptions.expires_at,subscriptions.source,subscriptions.status FROM saas_subscriptions subscriptions LEFT JOIN saas_subscription_daily_usage daily ON daily.subscription_id=subscriptions.id AND daily.day=? WHERE subscriptions.user_id=? AND (?=1 OR (subscriptions.status='active' AND subscriptions.expires_at>?)) ORDER BY subscriptions.expires_at,subscriptions.id")
        .bind(day).bind(user_id).bind(i64::from(all)).bind(repository::now()).fetch_all(pool).await.map_err(db_error)
}
fn row_json(row: &SubscriptionRow) -> Value {
    json!({"id":row.0,"planId":row.1,"planName":row.2,"dailyQuotaMicros":row.3,"todayUsedMicros":row.4,"todayFrozenMicros":row.5,"todayRemainingMicros":(row.3-row.4-row.5).max(0),"startsAt":repository::timestamp(row.6),"expiresAt":repository::timestamp(row.7),"source":row.8,"status":row.9})
}
pub async fn usage(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let (balance, frozen): (i64, i64) =
        sqlx::query_as("SELECT balance_micros,frozen_micros FROM saas_users WHERE id=?")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(db_error)?;
    let rows = active_rows(pool, user_id, false).await?;
    let subscription_remaining: i64 = rows.iter().map(|row| (row.3 - row.4 - row.5).max(0)).sum();
    let wallet_remaining = (balance - frozen).max(0);
    Ok(
        json!({"isValid":true,"mode":if rows.is_empty(){"balance"}else{"subscription"},"planName":if rows.is_empty(){"AI Switch SaaS 余额"}else{"AI Switch SaaS 每日订阅 + 余额"},"remaining":(subscription_remaining+wallet_remaining) as f64/1_000_000.0,"balance":wallet_remaining as f64/1_000_000.0,"unit":"USD","subscription":{"dailyRemainingMicros":subscription_remaining,"reset":"UTC 00:00","items":rows.iter().map(row_json).collect::<Vec<_>>()}}),
    )
}
pub async fn subscriptions(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let rows = active_rows(pool, user_id, true).await?;
    Ok(json!({"reset":"UTC 00:00","items":rows.iter().map(row_json).collect::<Vec<_>>() }))
}
