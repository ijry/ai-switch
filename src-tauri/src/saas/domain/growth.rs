use crate::error::AppError;
use crate::saas::{
    auth,
    billing::operations,
    config,
    repository::{self, db_error, invalid},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPlan {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub duration_days: i64,
    pub quota_micros: i64,
    pub price_micros: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserSubscription {
    pub id: String,
    pub user_id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub kind: String,
    pub quota_micros: i64,
    pub used_micros: i64,
    pub frozen_micros: i64,
    pub starts_at: i64,
    pub expires_at: i64,
    pub source: String,
    pub status: String,
}

fn plan_json(plan: &SubscriptionPlan) -> Value {
    json!({"id":plan.id,"name":plan.name,"kind":plan.kind,"durationDays":plan.duration_days,"dailyQuotaMicros":plan.quota_micros,"quotaMicros":plan.quota_micros,"priceMicros":plan.price_micros,"status":plan.status})
}

fn subscription_json(item: &UserSubscription) -> Value {
    json!({"id":item.id,"userId":item.user_id,"planId":item.plan_id,"planName":item.plan_name,"kind":item.kind,"dailyQuotaMicros":item.quota_micros,"quotaMicros":item.quota_micros,"todayUsedMicros":item.used_micros,"usedMicros":item.used_micros,"todayFrozenMicros":item.frozen_micros,"frozenMicros":item.frozen_micros,"todayAvailableMicros":(item.quota_micros-item.used_micros-item.frozen_micros).max(0),"availableMicros":(item.quota_micros-item.used_micros-item.frozen_micros).max(0),"startsAt":repository::timestamp(item.starts_at),"expiresAt":repository::timestamp(item.expires_at),"source":item.source,"status":item.status})
}

async fn load_plan(
    connection: &mut SqliteConnection,
    plan_id: &str,
    active_only: bool,
) -> Result<SubscriptionPlan, AppError> {
    sqlx::query_as("SELECT id,name,kind,duration_days,quota_micros,price_micros,status FROM saas_subscription_plans WHERE id=? AND (?=0 OR status='active')")
        .bind(plan_id).bind(i64::from(active_only)).fetch_optional(connection).await.map_err(db_error)?
        .ok_or_else(|| invalid("saas.subscription_plan_not_found", "Subscription plan is unavailable"))
}

pub(crate) async fn grant_connection(
    connection: &mut SqliteConnection,
    user_id: &str,
    plan_id: &str,
    source: &str,
) -> Result<Value, AppError> {
    let plan = load_plan(connection, plan_id, true).await?;
    let current = repository::now();
    let id = uuid::Uuid::new_v4().to_string();
    let expires_at = current
        .checked_add(
            plan.duration_days
                .checked_mul(86400)
                .ok_or_else(|| invalid("saas.validation", "Invalid plan duration"))?,
        )
        .ok_or_else(|| invalid("saas.validation", "Invalid plan duration"))?;
    sqlx::query("INSERT INTO saas_subscriptions(id,user_id,plan_id,plan_name,kind,quota_micros,starts_at,expires_at,source,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(user_id).bind(&plan.id).bind(&plan.name).bind(&plan.kind).bind(plan.quota_micros).bind(current).bind(expires_at).bind(source).bind(current).bind(current).execute(&mut *connection).await.map_err(db_error)?;
    let item: UserSubscription = sqlx::query_as("SELECT id,user_id,plan_id,plan_name,kind,quota_micros,0 AS used_micros,0 AS frozen_micros,starts_at,expires_at,source,status FROM saas_subscriptions WHERE id=?")
        .bind(&id).fetch_one(connection).await.map_err(db_error)?;
    Ok(subscription_json(&item))
}

pub async fn admin_plans(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let rows: Vec<SubscriptionPlan> = sqlx::query_as("SELECT id,name,kind,duration_days,quota_micros,price_micros,status FROM saas_subscription_plans ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(limit).bind(offset).fetch_all(pool).await.map_err(db_error)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_subscription_plans")
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    Ok(json!({"items":rows.iter().map(plan_json).collect::<Vec<_>>(),"total":total}))
}

pub async fn save_plan(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let name = repository::text(&payload, "name")?.trim();
    let kind = repository::text(&payload, "kind")?;
    if !["trial", "day", "week", "month", "quarter", "year"].contains(&kind) {
        return Err(invalid("saas.validation", "Invalid subscription plan kind"));
    }
    let duration_days = payload
        .get("durationDays")
        .and_then(Value::as_i64)
        .filter(|v| (1..=3660).contains(v))
        .ok_or_else(|| invalid("saas.validation", "Invalid plan duration"))?;
    let quota = payload
        .get("quotaMicros")
        .and_then(Value::as_i64)
        .filter(|v| (1..=repository::MAX_MONEY).contains(v))
        .ok_or_else(|| invalid("saas.validation", "Invalid plan quota"))?;
    let price = payload
        .get("priceMicros")
        .and_then(Value::as_i64)
        .filter(|v| (0..=repository::MAX_MONEY).contains(v))
        .ok_or_else(|| invalid("saas.validation", "Invalid plan price"))?;
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active");
    if !["active", "disabled"].contains(&status) {
        return Err(invalid("saas.validation", "Invalid plan status"));
    }
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = repository::now();
    let mut tx = repository::begin(pool).await?;
    sqlx::query("INSERT INTO saas_subscription_plans(id,name,kind,duration_days,quota_micros,price_micros,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,kind=excluded.kind,duration_days=excluded.duration_days,quota_micros=excluded.quota_micros,price_micros=excluded.price_micros,status=excluded.status,updated_at=excluded.updated_at")
        .bind(&id).bind(name).bind(kind).bind(duration_days).bind(quota).bind(price).bind(status).bind(now).bind(now).execute(&mut *tx).await.map_err(db_error)?;
    repository::audit(
        &mut tx,
        "subscriptions.plan.save",
        Some(&id),
        json!({"name":name,"kind":kind}),
    )
    .await?;
    let plan = load_plan(&mut tx, &id, false).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(plan_json(&plan))
}

pub async fn grant(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let user_id = repository::text(&payload, "userId")?;
    let plan_id = repository::text(&payload, "planId")?;
    let mut tx = repository::begin(pool).await?;
    repository::require_user(&mut tx, user_id).await?;
    let result = grant_connection(&mut tx, user_id, plan_id, "administrator").await?;
    repository::audit(
        &mut tx,
        "subscriptions.grant",
        Some(user_id),
        json!({"planId":plan_id}),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(result)
}

pub async fn user_list(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let rows: Vec<UserSubscription> = sqlx::query_as("SELECT subscriptions.id,subscriptions.user_id,subscriptions.plan_id,subscriptions.plan_name,subscriptions.kind,subscriptions.quota_micros,COALESCE(daily.used_micros,0) AS used_micros,COALESCE(daily.frozen_micros,0) AS frozen_micros,subscriptions.starts_at,subscriptions.expires_at,subscriptions.source,subscriptions.status FROM saas_subscriptions subscriptions LEFT JOIN saas_subscription_daily_usage daily ON daily.subscription_id=subscriptions.id AND daily.day=? WHERE subscriptions.user_id=? ORDER BY subscriptions.expires_at,subscriptions.id")
        .bind(day).bind(user_id).fetch_all(pool).await.map_err(db_error)?;
    Ok(json!({"items":rows.iter().map(subscription_json).collect::<Vec<_>>(),"total":rows.len()}))
}

pub async fn public_plans(pool: &SqlitePool) -> Result<Value, AppError> {
    let rows: Vec<SubscriptionPlan> = sqlx::query_as("SELECT id,name,kind,duration_days,quota_micros,price_micros,status FROM saas_subscription_plans WHERE status='active' ORDER BY duration_days,price_micros,id")
        .fetch_all(pool).await.map_err(db_error)?;
    Ok(json!({"items":rows.iter().map(plan_json).collect::<Vec<_>>(),"total":rows.len()}))
}

pub async fn purchase(pool: &SqlitePool, user_id: &str, payload: Value) -> Result<Value, AppError> {
    let plan_id = repository::text(&payload, "planId")?;
    let request_id = repository::text(&payload, "requestId")?;
    let mut tx = repository::begin(pool).await?;
    let plan = load_plan(&mut tx, plan_id, true).await?;
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if balance < plan.price_micros {
        return Err(invalid(
            "saas.insufficient_balance",
            "Insufficient available balance",
        ));
    }
    let after = repository::money(balance as i128 - plan.price_micros as i128)?;
    sqlx::query("UPDATE saas_users SET balance_micros=?,updated_at=? WHERE id=?")
        .bind(after)
        .bind(repository::now())
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("INSERT INTO saas_wallet_ledger(id,user_id,kind,amount_micros,balance_after_micros,source_id,idempotency_key,reason,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(user_id).bind("subscription_purchase").bind(-plan.price_micros).bind(after).bind(request_id).bind(format!("subscription_purchase:{user_id}:{request_id}")).bind(&plan.name).bind("user").bind(repository::now()).execute(&mut *tx).await.map_err(db_error)?;
    let result = grant_connection(&mut tx, user_id, plan_id, "purchase").await?;
    tx.commit().await.map_err(db_error)?;
    Ok(result)
}

pub async fn checkin(pool: &SqlitePool, user_id: &str) -> Result<Value, AppError> {
    let mut tx = repository::begin(pool).await?;
    let cfg = config::require_enabled(&mut tx).await?;
    if !cfg.checkin_enabled || cfg.checkin_reward_micros <= 0 {
        return Err(invalid(
            "saas.checkin_disabled",
            "Daily check-in is disabled",
        ));
    }
    let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO saas_checkins(user_id,day,amount_micros,created_at) VALUES(?,?,?,?)",
    )
    .bind(user_id)
    .bind(&day)
    .bind(cfg.checkin_reward_micros)
    .bind(repository::now())
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if inserted == 0 {
        return Err(invalid(
            "saas.already_checked_in",
            "You have already checked in today",
        ));
    }
    let source = format!("{user_id}:{day}");
    operations::credit_wallet(
        &mut tx,
        user_id,
        cfg.checkin_reward_micros,
        "checkin",
        &source,
        "user",
        Some("Daily check-in"),
    )
    .await?;
    let user = auth::safe_user(&mut tx, user_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(
        json!({"day":day,"amountMicros":cfg.checkin_reward_micros,"balanceMicros":user.balance_micros}),
    )
}

pub async fn admin_list(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let user_id = repository::text(&payload, "userId")?;
    user_list(pool, user_id).await
}

pub async fn cancel(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let user_id = repository::text(&payload, "userId")?;
    let id = repository::text(&payload, "id")?;
    let reason = repository::text(&payload, "reason")?.trim();
    if reason.is_empty() || reason.len() > 1024 {
        return Err(invalid(
            "saas.validation",
            "A cancellation reason is required",
        ));
    }
    let mut tx = repository::begin(pool).await?;
    let changed = sqlx::query("UPDATE saas_subscriptions SET status='cancelled',updated_at=? WHERE id=? AND user_id=? AND status='active'")
        .bind(repository::now()).bind(id).bind(user_id).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
    if changed == 0 {
        return Err(invalid(
            "saas.subscription_not_found",
            "Active subscription not found for this user",
        ));
    }
    repository::audit(
        &mut tx,
        "subscriptions.cancel",
        Some(id),
        json!({"userId":user_id,"reason":reason}),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"ok":true}))
}
