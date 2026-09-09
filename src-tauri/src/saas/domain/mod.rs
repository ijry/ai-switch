pub mod external;
pub mod groups;
pub mod growth;
pub mod invites;
pub mod keys;

use crate::error::AppError;
use crate::saas::{
    auth, billing, config,
    repository::{self, db_error, invalid},
};
pub use groups::{permitted_accounts, permitted_accounts_for_model};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub async fn admin(pool: &SqlitePool, operation: &str, payload: Value) -> Result<Value, AppError> {
    match operation {
        "overview" => billing::operations::overview(pool, None).await,
        "catalog" => groups::catalog(pool, payload).await,
        "users.list" => list_users(pool, payload).await,
        "users.create" => create_user(pool, payload).await,
        "users.credit" => billing::operations::admin_credit(pool, payload).await,
        "users.status" => user_status(pool, payload).await,
        "groups.list" => groups::list(pool, payload, false).await,
        "groups.available" => groups::available(pool, payload).await,
        "groups.save" => groups::save(pool, payload).await,
        "subscriptions.plans.list" => growth::admin_plans(pool, payload).await,
        "subscriptions.plans.save" => growth::save_plan(pool, payload).await,
        "subscriptions.grant" => growth::grant(pool, payload).await,
        "invites.codes.list" => invites::list_codes(pool, payload).await,
        "invites.codes.create" => invites::create_codes(pool, payload).await,
        "invites.codes.disable" => invites::disable_code(pool, payload).await,
        "invites.rewards.list" => invites::rewards(pool, None, payload).await,
        "invites.rewards.review" => invites::review(pool, payload).await,
        "recharges.list" => billing::operations::list_recharges(pool, None, payload).await,
        "recharges.review" => billing::operations::review_recharge(pool, payload).await,
        "codes.list" => billing::operations::list_codes(pool, payload).await,
        "codes.create" => billing::operations::create_codes(pool, payload).await,
        "codes.disable" => billing::operations::disable_code(pool, payload).await,
        "ledger.list" => billing::operations::ledger(pool, payload).await,
        "ledger.reconcile" => serde_json::to_value(billing::reconcile(pool, payload).await?)
            .map_err(|_| invalid("saas.serialization", "Could not encode settlement")),
        _ => Err(invalid(
            "saas.unknown_operation",
            "Unknown SaaS administrator operation",
        )),
    }
}

pub async fn user(
    pool: &SqlitePool,
    user_id: &str,
    operation: &str,
    payload: Value,
) -> Result<Value, AppError> {
    {
        let mut connection = pool.acquire().await.map_err(db_error)?;
        config::require_enabled(&mut connection).await?;
        repository::require_user(&mut connection, user_id).await?;
    }
    match operation {
        "overview" => billing::operations::overview(pool, Some(user_id)).await,
        "usage" => billing::operations::usage(pool, user_id, payload).await,
        "groups" => groups::list(pool, payload, true).await,
        "subscriptions.list" => growth::user_list(pool, user_id).await,
        "subscriptions.plans" => growth::public_plans(pool).await,
        "subscriptions.purchase" => growth::purchase(pool, user_id, payload).await,
        "checkin" => growth::checkin(pool, user_id).await,
        "invites.overview" => invites::overview(pool, user_id).await,
        "invites.rewards" => invites::rewards(pool, Some(user_id), payload).await,
        "external-key.status" => external::key_status(pool, user_id).await,
        "external-key.rotate" => external::rotate_key(pool, user_id).await,
        "external-key.revoke" => external::revoke_key(pool, user_id).await,
        "keys.list" => keys::list(pool, user_id, payload).await,
        "keys.create" => keys::create(pool, user_id, payload).await,
        "keys.update" => keys::update(pool, user_id, payload, false).await,
        "keys.rotate" => keys::update(pool, user_id, payload, true).await,
        "recharges.list" => billing::operations::list_recharges(pool, Some(user_id), payload).await,
        "recharges.create" => billing::operations::create_recharge(pool, user_id, payload).await,
        "recharges.cancel" => billing::operations::cancel_recharge(pool, user_id, payload).await,
        "redeem" => billing::operations::redeem(pool, user_id, payload).await,
        _ => Err(invalid(
            "saas.unknown_operation",
            "Unknown SaaS user operation",
        )),
    }
}

async fn list_users(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let query = payload.get("query").and_then(Value::as_str).unwrap_or("");
    if query.len() > 120 {
        return Err(invalid("saas.validation", "User search is too long"));
    }
    let status = payload.get("status").and_then(Value::as_str);
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_users WHERE (?='' OR instr(lower(login),lower(?))>0 OR instr(lower(COALESCE(email,'')),lower(?))>0 OR github_id=?) AND (? IS NULL OR status=?)")
        .bind(query).bind(query).bind(query).bind(query).bind(status).bind(status).fetch_one(&mut *transaction).await.map_err(db_error)?;
    let identifiers: Vec<String> = sqlx::query_scalar("SELECT id FROM saas_users WHERE (?='' OR instr(lower(login),lower(?))>0 OR instr(lower(COALESCE(email,'')),lower(?))>0 OR github_id=?) AND (? IS NULL OR status=?) ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(query).bind(query).bind(query).bind(query).bind(status).bind(status).bind(limit).bind(offset).fetch_all(&mut *transaction).await.map_err(db_error)?;
    let mut items = Vec::new();
    for identifier in identifiers {
        items.push(auth::safe_user(&mut transaction, &identifier).await?);
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":items,"total":total}))
}

async fn create_user(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let email = repository::text(&payload, "email")?;
    let password = repository::text(&payload, "password")?;
    serde_json::to_value(auth::create_password_user(pool, email, password).await?)
        .map_err(|_| invalid("saas.serialization", "Could not encode user"))
}

async fn user_status(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let user_id = repository::text(&payload, "id")?;
    let status = repository::text(&payload, "status")?;
    if !["active", "banned"].contains(&status) {
        return Err(invalid(
            "saas.validation",
            "User status must be active or banned",
        ));
    }
    let reason = payload.get("reason").and_then(Value::as_str).unwrap_or("");
    if reason.len() > 1024 {
        return Err(invalid("saas.validation", "Reason is too long"));
    }
    let mut transaction = repository::begin(pool).await?;
    let changed = sqlx::query("UPDATE saas_users SET status=?,updated_at=? WHERE id=?")
        .bind(status)
        .bind(repository::now())
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?
        .rows_affected();
    if changed == 0 {
        return Err(invalid("saas.user_not_found", "User does not exist"));
    }
    if status == "banned" {
        sqlx::query("UPDATE saas_sessions SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL")
            .bind(repository::now())
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
    }
    repository::audit(
        &mut transaction,
        "users.status",
        Some(user_id),
        json!({"status":status,"reason":reason}),
    )
    .await?;
    let result = auth::safe_user(&mut transaction, user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!(result))
}

#[cfg(test)]
mod tests;
