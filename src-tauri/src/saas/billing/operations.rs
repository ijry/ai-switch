use crate::error::AppError;
use crate::saas::{
    auth, config,
    repository::{self, db_error, invalid},
};
use serde_json::{json, Value};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

#[derive(FromRow)]
struct Recharge {
    id: String,
    user_id: String,
    request_id: String,
    amount_cny_fen: i64,
    exchange_rate_micros: i64,
    credit_micros: i64,
    status: String,
    note: Option<String>,
    reason: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl Recharge {
    fn json(&self) -> Value {
        json!({"id":self.id,"userId":self.user_id,"requestId":self.request_id,"amountCnyFen":self.amount_cny_fen,"exchangeRateMicros":self.exchange_rate_micros,
            "creditMicros":self.credit_micros,"status":self.status,"note":self.note,"reason":self.reason,"createdAt":repository::timestamp(self.created_at),"updatedAt":repository::timestamp(self.updated_at)})
    }
}

async fn recharge(
    connection: &mut SqliteConnection,
    identifier: &str,
    owner: Option<&str>,
) -> Result<Recharge, AppError> {
    sqlx::query_as("SELECT id,user_id,request_id,amount_cny_fen,exchange_rate_micros,credit_micros,status,note,reason,created_at,updated_at FROM saas_recharge_orders WHERE id=? AND (? IS NULL OR user_id=?)")
        .bind(identifier).bind(owner).bind(owner).fetch_optional(connection).await.map_err(db_error)?.ok_or_else(|| invalid("saas.order_not_found", "Recharge order is unavailable"))
}

fn optional_text(payload: &Value, field: &str) -> Result<Option<String>, AppError> {
    match payload.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.len() <= 1024 => Ok(Some(value.trim().into())),
        _ => Err(invalid("saas.validation", format!("Invalid {field}"))),
    }
}

pub(crate) async fn throttle(
    connection: &mut SqliteConnection,
    user_id: &str,
    operation: &str,
    limit: i64,
    window: i64,
) -> Result<(), AppError> {
    let key = format!("rate.{operation}.{user_id}");
    let current = repository::now();
    let stored: Option<(String, i64)> =
        sqlx::query_as("SELECT value_json,updated_at FROM saas_settings WHERE key=?")
            .bind(&key)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db_error)?;
    let (count, start) = match stored {
        Some((value, start)) if current - start < window => (
            value
                .parse::<i64>()
                .map_err(|_| invalid("saas.rate_limited", "Rate limit state is unavailable"))?,
            start,
        ),
        _ => (0, current),
    };
    if count >= limit {
        return Err(invalid(
            "saas.rate_limited",
            "Too many requests; please wait and try again",
        ));
    }
    sqlx::query("INSERT INTO saas_settings(key,value_json,updated_at) VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at")
        .bind(key).bind((count+1).to_string()).bind(start).execute(connection).await.map_err(db_error)?;
    Ok(())
}

pub async fn create_recharge(
    pool: &SqlitePool,
    user_id: &str,
    payload: Value,
) -> Result<Value, AppError> {
    let amount = payload
        .get("amountCnyFen")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("saas.validation", "amountCnyFen must be an integer"))?;
    let request_id = repository::text(&payload, "requestId")?;
    let note = optional_text(&payload, "note")?;
    let mut transaction = repository::begin(pool).await?;
    let config = config::require_enabled(&mut transaction).await?;
    repository::require_user(&mut transaction, user_id).await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM saas_recharge_orders WHERE user_id=? AND request_id=?")
            .bind(user_id)
            .bind(request_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(db_error)?;
    if let Some(identifier) = existing {
        let existing = recharge(&mut transaction, &identifier, Some(user_id)).await?;
        if existing.amount_cny_fen != amount || existing.note != note {
            return Err(invalid(
                "saas.idempotency_conflict",
                "This request identifier was already used for another recharge",
            ));
        }
        return Ok(existing.json());
    }
    let rate = config
        .exchange_rate_micros
        .ok_or_else(|| invalid("saas.exchange_rate", "Exchange rate is not configured"))?;
    let credit = super::pricing::recharge_credit(amount, rate)?;
    throttle(&mut transaction, user_id, "recharge", 20, 3600).await?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM saas_recharge_orders WHERE user_id=? AND status='pending'",
    )
    .bind(user_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(db_error)?;
    if pending >= 20 {
        return Err(invalid(
            "saas.rate_limited",
            "Too many pending recharge orders",
        ));
    }
    let identifier = uuid::Uuid::new_v4().to_string();
    let current = repository::now();
    sqlx::query("INSERT INTO saas_recharge_orders(id,user_id,request_id,amount_cny_fen,exchange_rate_micros,credit_micros,note,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(&identifier).bind(user_id).bind(request_id).bind(amount).bind(rate).bind(credit).bind(note).bind(current).bind(current).execute(&mut *transaction).await.map_err(db_error)?;
    let result = recharge(&mut transaction, &identifier, Some(user_id))
        .await?
        .json();
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

pub async fn list_recharges(
    pool: &SqlitePool,
    owner: Option<&str>,
    payload: Value,
) -> Result<Value, AppError> {
    let user_id = owner.or_else(|| {
        payload
            .get("userId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    });
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let (limit, offset) = repository::page(&payload)?;
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_recharge_orders WHERE (? IS NULL OR user_id=?) AND (? IS NULL OR status=?)")
        .bind(user_id).bind(user_id).bind(status).bind(status).fetch_one(&mut *transaction).await.map_err(db_error)?;
    let rows: Vec<Recharge> = sqlx::query_as("SELECT id,user_id,request_id,amount_cny_fen,exchange_rate_micros,credit_micros,status,note,reason,created_at,updated_at FROM saas_recharge_orders WHERE (? IS NULL OR user_id=?) AND (? IS NULL OR status=?) ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(user_id).bind(user_id).bind(status).bind(status).bind(limit).bind(offset).fetch_all(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":rows.iter().map(Recharge::json).collect::<Vec<_>>(),"total":total}))
}

pub async fn cancel_recharge(
    pool: &SqlitePool,
    user_id: &str,
    payload: Value,
) -> Result<Value, AppError> {
    let identifier = repository::text(&payload, "id")?;
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    repository::require_user(&mut transaction, user_id).await?;
    let order = recharge(&mut transaction, identifier, Some(user_id)).await?;
    if order.status == "cancelled" {
        return Ok(order.json());
    }
    if order.status != "pending" {
        return Err(invalid(
            "saas.order_final",
            "Only pending orders can be cancelled",
        ));
    }
    sqlx::query("UPDATE saas_recharge_orders SET status='cancelled',updated_at=? WHERE id=? AND user_id=? AND status='pending'")
        .bind(repository::now()).bind(identifier).bind(user_id).execute(&mut *transaction).await.map_err(db_error)?;
    let result = recharge(&mut transaction, identifier, Some(user_id))
        .await?
        .json();
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

pub(crate) async fn credit_wallet(
    connection: &mut SqliteConnection,
    user_id: &str,
    amount: i64,
    kind: &str,
    source: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    if amount <= 0 {
        return Err(invalid("saas.amount", "Credit must be positive"));
    }
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(user_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db_error)?;
    let after = repository::money(balance as i128 + amount as i128)?;
    sqlx::query("UPDATE saas_users SET balance_micros=?,updated_at=? WHERE id=?")
        .bind(after)
        .bind(repository::now())
        .bind(user_id)
        .execute(&mut *connection)
        .await
        .map_err(db_error)?;
    sqlx::query("INSERT INTO saas_wallet_ledger(id,user_id,kind,amount_micros,balance_after_micros,source_id,idempotency_key,reason,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(user_id).bind(kind).bind(amount).bind(after).bind(source).bind(format!("{kind}:{source}")).bind(reason).bind(actor).bind(repository::now()).execute(connection).await.map_err(db_error)?;
    Ok(())
}

pub async fn admin_credit(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let user_id = repository::text(&payload, "userId")?.trim();
    let reason = repository::text(&payload, "reason")?.trim();
    let amount = payload
        .get("amountMicros")
        .and_then(Value::as_i64)
        .filter(|amount| (1..=repository::MAX_MONEY).contains(amount))
        .ok_or_else(|| invalid("saas.validation", "amountMicros must be a positive integer"))?;
    let mut transaction = repository::begin(pool).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM saas_users WHERE id=?)")
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    if !exists {
        return Err(invalid("saas.user_not_found", "User does not exist"));
    }
    let source = uuid::Uuid::new_v4().to_string();
    credit_wallet(
        &mut transaction,
        user_id,
        amount,
        "recharge",
        &source,
        "administrator",
        Some(reason),
    )
    .await?;
    crate::saas::domain::invites::create_recharge_reward(
        &mut transaction,
        user_id,
        &source,
        amount,
    )
    .await?;
    repository::audit(
        &mut transaction,
        "users.credit",
        Some(user_id),
        json!({"amountMicros":amount,"reason":reason,"sourceId":source}),
    )
    .await?;
    let result = auth::safe_user(&mut transaction, user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    serde_json::to_value(result).map_err(|_| invalid("saas.serialization", "Could not encode user"))
}

pub async fn review_recharge(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let identifier = repository::text(&payload, "id")?;
    let status = repository::text(&payload, "status")?;
    let reason = repository::text(&payload, "reason")?;
    if !["approved", "rejected"].contains(&status) {
        return Err(invalid(
            "saas.validation",
            "Review status must be approved or rejected",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    let order = recharge(&mut transaction, identifier, None).await?;
    if order.status == status {
        return Ok(order.json());
    }
    if order.status != "pending" {
        return Err(invalid(
            "saas.order_final",
            "This recharge order has already been finalized",
        ));
    }
    sqlx::query("UPDATE saas_recharge_orders SET status=?,reason=?,reviewed_by='administrator',updated_at=? WHERE id=? AND status='pending'")
        .bind(status).bind(reason).bind(repository::now()).bind(identifier).execute(&mut *transaction).await.map_err(db_error)?;
    if status == "approved" {
        credit_wallet(
            &mut transaction,
            &order.user_id,
            order.credit_micros,
            "recharge",
            identifier,
            "administrator",
            Some(reason),
        )
        .await?;
        crate::saas::domain::invites::create_recharge_reward(
            &mut transaction,
            &order.user_id,
            identifier,
            order.credit_micros,
        )
        .await?;
    }
    repository::audit(
        &mut transaction,
        "recharges.review",
        Some(identifier),
        json!({"status":status,"reason":reason,"creditMicros":order.credit_micros}),
    )
    .await?;
    let result = recharge(&mut transaction, identifier, None).await?.json();
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

#[derive(FromRow)]
struct RedeemCode {
    id: String,
    batch_id: String,
    prefix: String,
    suffix: String,
    amount_micros: i64,
    subscription_plan_id: Option<String>,
    status: String,
    expires_at: Option<i64>,
    used_by: Option<String>,
    used_at: Option<i64>,
    created_at: i64,
}

impl RedeemCode {
    fn json(self) -> Value {
        json!({"id":self.id,"batchId":self.batch_id,"prefix":self.prefix,"suffix":self.suffix,"amountMicros":self.amount_micros,"subscriptionPlanId":self.subscription_plan_id,"status":self.status,
            "expiresAt":self.expires_at.map(repository::timestamp),"usedBy":self.used_by,"usedAt":self.used_at.map(repository::timestamp),"createdAt":repository::timestamp(self.created_at)})
    }
}

pub async fn create_codes(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let count = payload
        .get("count")
        .and_then(Value::as_i64)
        .filter(|count| (1..=100).contains(count))
        .ok_or_else(|| invalid("saas.validation", "count must be between 1 and 100"))?;
    let amount = payload
        .get("amountMicros")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let subscription_plan_id = payload
        .get("subscriptionPlanId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !(0..=repository::MAX_MONEY).contains(&amount)
        || (amount == 0 && subscription_plan_id.is_none())
    {
        return Err(invalid(
            "saas.validation",
            "A positive amountMicros or subscription plan is required",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    if let Some(plan_id) = subscription_plan_id {
        let plan_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM saas_subscription_plans WHERE id=? AND status='active')",
        )
        .bind(plan_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
        if !plan_exists {
            return Err(invalid(
                "saas.subscription_plan_not_found",
                "Subscription plan is unavailable",
            ));
        }
    }
    let expires = repository::parse_expiry(payload.get("expiresAt"))?;
    let batch = uuid::Uuid::new_v4().to_string();
    let mut items = Vec::new();
    for _ in 0..count {
        let identifier = uuid::Uuid::new_v4().to_string();
        let plaintext = repository::random_secret("sc-saas-");
        let prefix = &plaintext[..16];
        let suffix = &plaintext[plaintext.len() - 4..];
        sqlx::query("INSERT INTO saas_redeem_codes(id,batch_id,token_hash,prefix,suffix,amount_micros,subscription_plan_id,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&identifier).bind(&batch).bind(repository::hash_secret(&plaintext)).bind(prefix).bind(suffix).bind(amount).bind(subscription_plan_id).bind(expires).bind(repository::now()).execute(&mut *transaction).await.map_err(db_error)?;
        items.push(json!({"id":identifier,"batchId":batch,"plaintextCode":plaintext,"prefix":prefix,"suffix":suffix,"amountMicros":amount,"subscriptionPlanId":subscription_plan_id,"status":"active","expiresAt":expires.map(repository::timestamp)}));
    }
    repository::audit(
        &mut transaction,
        "codes.create",
        Some(&batch),
        json!({"count":count,"amountMicros":amount,"subscriptionPlanId":subscription_plan_id}),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":items,"total":count,"batchId":batch}))
}

pub async fn list_codes(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let batch = payload.get("batchId").and_then(Value::as_str);
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM saas_redeem_codes WHERE (? IS NULL OR status=?) AND (? IS NULL OR batch_id=?)")
        .bind(status).bind(status).bind(batch).bind(batch).fetch_one(&mut *transaction).await.map_err(db_error)?;
    let rows: Vec<RedeemCode> = sqlx::query_as("SELECT id,batch_id,prefix,suffix,amount_micros,subscription_plan_id,status,expires_at,used_by,used_at,created_at FROM saas_redeem_codes WHERE (? IS NULL OR status=?) AND (? IS NULL OR batch_id=?) ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
        .bind(status).bind(status).bind(batch).bind(batch).bind(limit).bind(offset).fetch_all(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":rows.into_iter().map(RedeemCode::json).collect::<Vec<_>>(),"total":total}))
}

pub async fn disable_code(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let identifier = repository::text(&payload, "id")?;
    let mut transaction = repository::begin(pool).await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM saas_redeem_codes WHERE id=?")
            .bind(identifier)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(db_error)?;
    match status.as_deref() {
        Some("active") | Some("disabled") => (),
        _ => {
            return Err(invalid(
                "saas.code_unavailable",
                "Code is unavailable or already redeemed",
            ))
        }
    }
    sqlx::query("UPDATE saas_redeem_codes SET status='disabled' WHERE id=? AND status='active'")
        .bind(identifier)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    repository::audit(
        &mut transaction,
        "codes.disable",
        Some(identifier),
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"id":identifier,"status":"disabled"}))
}

pub async fn redeem(pool: &SqlitePool, user_id: &str, payload: Value) -> Result<Value, AppError> {
    let code = repository::text(&payload, "code")?.trim();
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    repository::require_user(&mut transaction, user_id).await?;
    throttle(&mut transaction, user_id, "redeem", 20, 60).await?;
    let redeemed: Option<(String,i64,Option<String>)> = sqlx::query_as("UPDATE saas_redeem_codes SET status='redeemed',used_by=?,used_at=? WHERE token_hash=? AND status='active' AND (expires_at IS NULL OR expires_at>?) RETURNING id,amount_micros,subscription_plan_id")
        .bind(user_id).bind(repository::now()).bind(repository::hash_secret(code)).bind(repository::now()).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let Some((identifier, amount, subscription_plan_id)) = redeemed else {
        transaction.commit().await.map_err(db_error)?;
        return Err(invalid(
            "saas.code_unavailable",
            "Code is invalid, expired, disabled or already redeemed",
        ));
    };
    if amount > 0 {
        credit_wallet(
            &mut transaction,
            user_id,
            amount,
            "redeem",
            &identifier,
            user_id,
            None,
        )
        .await?;
    }
    let subscription = if let Some(plan_id) = subscription_plan_id.as_deref() {
        Some(
            crate::saas::domain::growth::grant_connection(
                &mut transaction,
                user_id,
                plan_id,
                "redeem",
            )
            .await?,
        )
    } else {
        None
    };
    let user = auth::safe_user(&mut transaction, user_id).await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(
        json!({"id":identifier,"amountMicros":amount,"balanceMicros":user.balance_micros,"subscription":subscription,"status":"redeemed"}),
    )
}

#[derive(FromRow)]
struct LedgerRow {
    id: String,
    user_id: String,
    kind: String,
    amount_micros: Option<i64>,
    balance_after_micros: Option<i64>,
    source_id: String,
    request_id: Option<String>,
    status: String,
    reserved_micros: i64,
    reason: Option<String>,
    actor: String,
    created_at: i64,
}

const LEDGER_VIEW: &str = "WITH entries AS (SELECT ledger.id,ledger.user_id,ledger.kind,ledger.amount_micros,ledger.balance_after_micros,ledger.source_id,ledger.request_id,COALESCE(reservation.status,'settled') AS status,COALESCE(reservation.reserved_micros,0) AS reserved_micros,ledger.reason,ledger.actor,ledger.created_at FROM saas_wallet_ledger ledger LEFT JOIN saas_billing_reservations reservation ON reservation.request_id=ledger.request_id UNION ALL SELECT request_id,user_id,'reservation',NULL,NULL,request_id,request_id,status,reserved_micros,NULL,'proxy',created_at FROM saas_billing_reservations WHERE status IN ('reserved','pending_review')) ";

pub async fn ledger(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let owner = payload
        .get("userId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let count_sql = format!("{LEDGER_VIEW} SELECT COUNT(*) FROM entries WHERE (? IS NULL OR user_id=?) AND (? IS NULL OR status=?)");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(owner)
        .bind(owner)
        .bind(status)
        .bind(status)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    let select_sql = format!("{LEDGER_VIEW} SELECT * FROM entries WHERE (? IS NULL OR user_id=?) AND (? IS NULL OR status=?) ORDER BY created_at DESC,id LIMIT ? OFFSET ?");
    let rows: Vec<LedgerRow> = sqlx::query_as(&select_sql)
        .bind(owner)
        .bind(owner)
        .bind(status)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(db_error)?;
    let items: Vec<Value> = rows.into_iter().map(|row| json!({"id":row.id,"userId":row.user_id,"kind":row.kind,"amountMicros":row.amount_micros,"balanceAfterMicros":row.balance_after_micros,"sourceId":row.source_id,"requestId":row.request_id,"status":row.status,"reservedMicros":row.reserved_micros,"reason":row.reason,"actor":row.actor,"createdAt":repository::timestamp(row.created_at)})).collect();
    transaction.commit().await.map_err(db_error)?;
    Ok(json!({"items":items,"total":total}))
}

#[derive(FromRow)]
struct UsageRow {
    user_id: String,
    key_id: String,
    group_id: String,
    model: String,
    hour: i64,
    request_count: i64,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    output_tokens: i64,
    cost_micros: i64,
    unconfirmed_usage_count: i64,
}

#[derive(Default, FromRow, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageAggregate {
    request_count: i64,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    output_tokens: i64,
    cost_micros: i64,
    unconfirmed_usage_count: i64,
}

const USAGE_SUMS:&str = "COALESCE(SUM(request_count),0) AS request_count,COALESCE(SUM(input_tokens),0) AS input_tokens,COALESCE(SUM(cache_read_tokens),0) AS cache_read_tokens,COALESCE(SUM(cache_write_tokens),0) AS cache_write_tokens,COALESCE(SUM(output_tokens),0) AS output_tokens,COALESCE(SUM(cost_micros),0) AS cost_micros,COALESCE(SUM(unconfirmed_usage_count),0) AS unconfirmed_usage_count";

fn date_filter(payload: &Value, key: &str, fallback: i64) -> Result<i64, AppError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(fallback),
        Some(Value::String(value)) => chrono::DateTime::parse_from_rfc3339(value)
            .map(|value| value.timestamp())
            .map_err(|_| {
                invalid(
                    "saas.validation",
                    format!("{key} must be an RFC3339 timestamp"),
                )
            }),
        _ => Err(invalid("saas.validation", format!("Invalid {key}"))),
    }
}

pub async fn usage(pool: &SqlitePool, user_id: &str, payload: Value) -> Result<Value, AppError> {
    let (limit, offset) = repository::page(&payload)?;
    let current = repository::now();
    let from = date_filter(&payload, "from", current - 30 * 86400)?;
    let to = date_filter(&payload, "to", current + 3600)?;
    if from >= to || to - from > 366 * 86400 {
        return Err(invalid(
            "saas.validation",
            "Usage date range must be at most 366 days",
        ));
    }
    let key = payload
        .get("keyId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let group = payload
        .get("groupId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let filter = " FROM saas_usage_hourly WHERE user_id=? AND hour>=? AND hour<? AND (? IS NULL OR key_id=?) AND (? IS NULL OR group_id=?) AND (? IS NULL OR model=?)";
    let count_sql = format!("SELECT COUNT(*){filter}");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(user_id)
        .bind(from / 3600 * 3600)
        .bind(to)
        .bind(key)
        .bind(key)
        .bind(group)
        .bind(group)
        .bind(model)
        .bind(model)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    let select_sql = format!("SELECT user_id,key_id,group_id,model,hour,request_count,input_tokens,cache_read_tokens,cache_write_tokens,output_tokens,cost_micros,unconfirmed_usage_count{filter} ORDER BY hour DESC,key_id,model LIMIT ? OFFSET ?");
    let rows: Vec<UsageRow> = sqlx::query_as(&select_sql)
        .bind(user_id)
        .bind(from / 3600 * 3600)
        .bind(to)
        .bind(key)
        .bind(key)
        .bind(group)
        .bind(group)
        .bind(model)
        .bind(model)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await
        .map_err(db_error)?;
    let items: Vec<Value> = rows.into_iter().map(|row| json!({"userId":row.user_id,"keyId":row.key_id,"groupId":row.group_id,"model":row.model,"hour":repository::timestamp(row.hour),"requestCount":row.request_count,"inputTokens":row.input_tokens,"cacheReadTokens":row.cache_read_tokens,"cacheWriteTokens":row.cache_write_tokens,"outputTokens":row.output_tokens,"costMicros":row.cost_micros,"unconfirmedUsageCount":row.unconfirmed_usage_count})).collect();
    let totals_sql = format!("SELECT {USAGE_SUMS}{filter}");
    let totals: UsageAggregate = sqlx::query_as(&totals_sql)
        .bind(user_id)
        .bind(from / 3600 * 3600)
        .bind(to)
        .bind(key)
        .bind(key)
        .bind(group)
        .bind(group)
        .bind(model)
        .bind(model)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
    let days_sql = format!("SELECT hour/86400*86400 AS day{filter} GROUP BY day ORDER BY day");
    let days: Vec<i64> = sqlx::query_scalar(&days_sql)
        .bind(user_id)
        .bind(from / 3600 * 3600)
        .bind(to)
        .bind(key)
        .bind(key)
        .bind(group)
        .bind(group)
        .bind(model)
        .bind(model)
        .fetch_all(&mut *transaction)
        .await
        .map_err(db_error)?;
    let mut buckets = Vec::new();
    for day in days {
        let sql = format!("SELECT {USAGE_SUMS}{filter} AND hour>=? AND hour<?");
        let day_usage: UsageAggregate = sqlx::query_as(&sql)
            .bind(user_id)
            .bind(from / 3600 * 3600)
            .bind(to)
            .bind(key)
            .bind(key)
            .bind(group)
            .bind(group)
            .bind(model)
            .bind(model)
            .bind(day)
            .bind(day + 86400)
            .fetch_one(&mut *transaction)
            .await
            .map_err(db_error)?;
        let mut bucket = json!(day_usage);
        bucket["date"] = json!(repository::timestamp(day));
        buckets.push(bucket);
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(
        json!({"items":items,"total":total,"totals":totals,"buckets":buckets,"from":repository::timestamp(from),"to":repository::timestamp(to)}),
    )
}

async fn usage_summary(
    connection: &mut SqliteConnection,
    owner: Option<&str>,
    from: i64,
) -> Result<Value, AppError> {
    let totals: (i64,i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT COALESCE(SUM(request_count),0),COALESCE(SUM(input_tokens),0),COALESCE(SUM(cache_read_tokens),0),COALESCE(SUM(cache_write_tokens),0),COALESCE(SUM(output_tokens),0),COALESCE(SUM(cost_micros),0),COALESCE(SUM(unconfirmed_usage_count),0) FROM saas_usage_hourly WHERE (? IS NULL OR user_id=?) AND hour>=?")
        .bind(owner).bind(owner).bind(from).fetch_one(connection).await.map_err(db_error)?;
    Ok(
        json!({"requestCount":totals.0,"inputTokens":totals.1,"cacheReadTokens":totals.2,"cacheWriteTokens":totals.3,"outputTokens":totals.4,"costMicros":totals.5,"unconfirmedUsageCount":totals.6}),
    )
}

pub async fn overview(pool: &SqlitePool, user_id: Option<&str>) -> Result<Value, AppError> {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    let today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| invalid("saas.date", "Invalid current date"))?
        .and_utc()
        .timestamp();
    let month = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .ok_or_else(|| invalid("saas.date", "Invalid current month"))?
        .and_utc()
        .timestamp();
    let mut transaction = pool.begin().await.map_err(db_error)?;
    let today_usage = usage_summary(&mut transaction, user_id, today).await?;
    let month_usage = usage_summary(&mut transaction, user_id, month).await?;
    let (pending_count,pending_micros): (i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(reserved_micros),0) FROM saas_billing_reservations WHERE status='pending_review' AND (? IS NULL OR user_id=?)")
        .bind(user_id).bind(user_id).fetch_one(&mut *transaction).await.map_err(db_error)?;
    let config = config::load_connection(&mut transaction).await?;
    let mut result = json!({"today":today_usage,"month":month_usage,"pendingReviewCount":pending_count,"pendingReviewMicros":pending_micros,"exchangeRateMicros":config.exchange_rate_micros});
    if let Some(user_id) = user_id {
        repository::require_user(&mut transaction, user_id).await?;
        let user = auth::safe_user(&mut transaction, user_id).await?;
        result["balanceMicros"] = json!(user.balance_micros);
        result["frozenMicros"] = json!(user.frozen_micros);
        result["availableMicros"] = json!(user.available_micros);
        result["debtMicros"] = json!(user.debt_micros);
        result["user"] = json!(user);
        let redemptions:Vec<(String,i64,i64)> = sqlx::query_as("SELECT id,amount_micros,created_at FROM saas_wallet_ledger WHERE user_id=? AND kind='redeem' ORDER BY created_at DESC,id LIMIT 20")
            .bind(user_id).fetch_all(&mut *transaction).await.map_err(db_error)?;
        result["redemptionHistory"] = json!(redemptions.into_iter().map(|(id,amount,created)| json!({"id":id,"userId":user_id,"kind":"redeem","amountMicros":amount,"createdAt":repository::timestamp(created)})).collect::<Vec<_>>());
    } else {
        let users: (i64,i64,i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(balance_micros),0),COALESCE(SUM(frozen_micros),0),COALESCE(SUM(CASE WHEN balance_micros<0 THEN -balance_micros ELSE 0 END),0) FROM saas_users").fetch_one(&mut *transaction).await.map_err(db_error)?;
        let groups: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM route_pool_groups WHERE platform IN ('codex','claude') AND deleted_at IS NULL")
                .fetch_one(&mut *transaction)
                .await
                .map_err(db_error)?;
        let orders: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM saas_recharge_orders WHERE status='pending'")
                .fetch_one(&mut *transaction)
                .await
                .map_err(db_error)?;
        result["userCount"] = json!(users.0);
        result["balanceMicros"] = json!(users.1);
        result["frozenMicros"] = json!(users.2);
        result["debtMicros"] = json!(users.3);
        result["groupCount"] = json!(groups);
        result["pendingRechargeCount"] = json!(orders);
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}
