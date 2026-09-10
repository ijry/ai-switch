pub mod operations;
pub mod pricing;

use crate::error::AppError;
use crate::saas::{
    config,
    domain::groups,
    repository::{self, db_error, invalid},
};
pub use pricing::{BillableUsage, ModelPrice};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, SqliteConnection, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiPrincipal {
    pub user_id: String,
    pub key_id: String,
    pub group_id: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
    pub request_id: String,
    pub user_id: String,
    pub key_id: String,
    pub group_id: String,
    pub platform: String,
    pub model: String,
    pub upstream_model: String,
    pub credential_ids: Vec<String>,
    pub reserved_micros: i64,
    pub price: ModelPrice,
    pub max_output_tokens: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settlement {
    pub request_id: String,
    pub user_id: String,
    pub key_id: String,
    pub group_id: String,
    pub model: String,
    pub status: String,
    pub price_usd_micros: Option<i64>,
    pub reserved_micros: i64,
}

#[derive(FromRow)]
struct Limits {
    balance_micros: i64,
    user_frozen: i64,
    spent_micros: i64,
    key_frozen: i64,
    limit_micros: Option<i64>,
    multiplier_micros: i64,
    max_output_tokens: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    allow_subscription: bool,
    allow_balance: bool,
}

#[derive(FromRow)]
pub(crate) struct StoredReservation {
    request_id: String,
    user_id: String,
    key_id: String,
    group_id: String,
    model: String,
    price_json: String,
    reserved_micros: i64,
    status: String,
    price_usd_micros: Option<i64>,
    funding_type: String,
    subscription_id: Option<String>,
    created_at: i64,
}

impl StoredReservation {
    fn result(&self) -> Settlement {
        Settlement {
            request_id: self.request_id.clone(),
            user_id: self.user_id.clone(),
            key_id: self.key_id.clone(),
            group_id: self.group_id.clone(),
            model: self.model.clone(),
            status: self.status.clone(),
            price_usd_micros: self.price_usd_micros,
            reserved_micros: self.reserved_micros,
        }
    }
}

pub async fn authenticate_key(
    pool: &SqlitePool,
    plaintext_key: &str,
) -> Result<ApiPrincipal, AppError> {
    if !plaintext_key.starts_with("sk-saas-") || plaintext_key.len() != 72 {
        return Err(invalid("saas.invalid_key", "Invalid API key"));
    }
    let mut transaction = pool.begin().await.map_err(db_error)?;
    config::require_enabled(&mut transaction).await?;
    let principal: Option<ApiPrincipal> = sqlx::query_as("SELECT keys.user_id,keys.id AS key_id,keys.group_id,groups.platform FROM saas_api_keys keys JOIN saas_users users ON users.id=keys.user_id JOIN route_pool_groups groups ON groups.id=keys.group_id WHERE keys.token_hash=? AND keys.status='active' AND (keys.expires_at IS NULL OR keys.expires_at>?) AND users.status='active' AND groups.platform IN ('codex','claude','gemini') AND groups.is_internal=0 AND groups.deleted_at IS NULL")
        .bind(repository::hash_secret(plaintext_key)).bind(repository::now()).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    principal.ok_or_else(|| {
        invalid(
            "saas.invalid_key",
            "API key is invalid, expired or unavailable",
        )
    })
}

pub async fn reserve_image(
    pool: &SqlitePool,
    principal: &ApiPrincipal,
    model: &str,
    image_count: i64,
) -> Result<Reservation, AppError> {
    if !(1..=10).contains(&image_count) || model.is_empty() || model.len() > 256 {
        return Err(invalid(
            "saas.request_limits",
            "Invalid image model or count",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    let limits: Option<Limits> = sqlx::query_as("SELECT users.balance_micros,users.frozen_micros AS user_frozen,keys.spent_micros,keys.frozen_micros AS key_frozen,keys.limit_micros,settings.multiplier_micros,settings.max_output_tokens,settings.timeout_seconds,settings.max_concurrency,settings.allow_subscription,settings.allow_balance FROM saas_api_keys keys JOIN saas_users users ON users.id=keys.user_id JOIN route_pool_groups groups ON groups.id=keys.group_id JOIN saas_group_settings settings ON settings.group_id=groups.id WHERE keys.id=? AND keys.user_id=? AND keys.group_id=? AND groups.platform=? AND keys.status='active' AND (keys.expires_at IS NULL OR keys.expires_at>?) AND users.status='active' AND groups.platform IN ('codex','gemini') AND groups.is_internal=0 AND groups.deleted_at IS NULL")
        .bind(&principal.key_id).bind(&principal.user_id).bind(&principal.group_id).bind(&principal.platform).bind(repository::now()).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let limits =
        limits.ok_or_else(|| invalid("saas.invalid_key", "API key is no longer available"))?;
    let priced: Option<(i64, String)> = sqlx::query_as("SELECT image_price_micros,upstream_model FROM saas_group_models WHERE group_id=? AND model=?")
        .bind(&principal.group_id).bind(model).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let (image_price_micros, upstream_model) =
        priced.filter(|(price, _)| *price > 0).ok_or_else(|| {
            invalid(
                "saas.model_not_allowed",
                "Image model is not priced and allowed by this group",
            )
        })?;
    let credential_ids = groups::permitted_accounts_for_capability_connection(
        &mut transaction,
        &principal.group_id,
        model,
        "image.generate",
    )
    .await?;
    if credential_ids.is_empty() {
        return Err(invalid(
            "saas.empty_pool",
            "No permitted account currently supports image generation",
        ));
    }
    let reserved_micros = repository::money(
        image_price_micros as i128 * image_count as i128 * limits.multiplier_micros as i128
            / 1_000_000_i128,
    )?;
    let reservation = reserve_fixed(
        &mut transaction,
        principal,
        model,
        upstream_model,
        credential_ids,
        reserved_micros,
        limits,
        serde_json::json!({"kind":"image","unitPriceMicros":image_price_micros,"count":image_count}).to_string(),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(reservation)
}

pub async fn reserve(
    pool: &SqlitePool,
    principal: &ApiPrincipal,
    model: &str,
    input_estimate: i64,
    output_limit: i64,
) -> Result<Reservation, AppError> {
    if !(0..=1_000_000).contains(&input_estimate)
        || !(1..=1_000_000).contains(&output_limit)
        || model.is_empty()
        || model.len() > 256
    {
        return Err(invalid(
            "saas.request_limits",
            "Invalid model or token limits",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    config::require_enabled(&mut transaction).await?;
    let limits: Option<Limits> = sqlx::query_as("SELECT users.balance_micros,users.frozen_micros AS user_frozen,keys.spent_micros,keys.frozen_micros AS key_frozen,keys.limit_micros,settings.multiplier_micros,settings.max_output_tokens,settings.timeout_seconds,settings.max_concurrency,settings.allow_subscription,settings.allow_balance FROM saas_api_keys keys JOIN saas_users users ON users.id=keys.user_id JOIN route_pool_groups groups ON groups.id=keys.group_id JOIN saas_group_settings settings ON settings.group_id=groups.id WHERE keys.id=? AND keys.user_id=? AND keys.group_id=? AND groups.platform=? AND keys.status='active' AND (keys.expires_at IS NULL OR keys.expires_at>?) AND users.status='active' AND groups.platform IN ('codex','claude','gemini') AND groups.is_internal=0 AND groups.deleted_at IS NULL")
        .bind(&principal.key_id).bind(&principal.user_id).bind(&principal.group_id).bind(&principal.platform).bind(repository::now()).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let limits =
        limits.ok_or_else(|| invalid("saas.invalid_key", "API key is no longer available"))?;
    if output_limit > limits.max_output_tokens {
        return Err(invalid(
            "saas.request_limits",
            "Requested output exceeds the group limit",
        ));
    }
    let model_price: Option<(i64,i64,i64,String)> = sqlx::query_as("SELECT input_price_micros,cache_price_micros,output_price_micros,upstream_model FROM saas_group_models WHERE group_id=? AND model=?")
        .bind(&principal.group_id).bind(model).fetch_optional(&mut *transaction).await.map_err(db_error)?;
    let (input_price_micros, cache_price_micros, output_price_micros, upstream_model) = model_price
        .ok_or_else(|| {
            invalid(
                "saas.model_not_allowed",
                "Model is not priced and allowed by this group",
            )
        })?;
    let credential_ids =
        groups::permitted_accounts_connection(&mut transaction, &principal.group_id, Some(model))
            .await?;
    if credential_ids.is_empty() {
        return Err(invalid(
            "saas.empty_pool",
            "No permitted account currently supports this model",
        ));
    }
    let price = ModelPrice {
        input_price_micros,
        cache_price_micros,
        output_price_micros,
        multiplier_micros: limits.multiplier_micros,
    };
    let mut conservative_price = price.clone();
    conservative_price.input_price_micros = input_price_micros.max(cache_price_micros);
    let reserved_micros = conservative_price.charge(&BillableUsage {
        input_tokens: input_estimate,
        output_tokens: output_limit,
        ..Default::default()
    })?;
    let subscription_day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let subscription_id: Option<String> = if limits.allow_subscription {
        sqlx::query_scalar("SELECT subscriptions.id FROM saas_subscriptions subscriptions LEFT JOIN saas_subscription_daily_usage daily ON daily.subscription_id=subscriptions.id AND daily.day=? WHERE subscriptions.user_id=? AND subscriptions.status='active' AND subscriptions.starts_at<=? AND subscriptions.expires_at>? AND subscriptions.quota_micros-COALESCE(daily.used_micros,0)-COALESCE(daily.frozen_micros,0)>=? ORDER BY subscriptions.expires_at,subscriptions.created_at,subscriptions.id LIMIT 1")
            .bind(&subscription_day).bind(&principal.user_id).bind(repository::now()).bind(repository::now()).bind(reserved_micros).fetch_optional(&mut *transaction).await.map_err(db_error)?
    } else {
        None
    };
    if reserved_micros > 0
        && subscription_id.is_none()
        && (!limits.allow_balance
            || limits.balance_micros as i128 - (limits.user_frozen as i128)
                < reserved_micros as i128)
    {
        return Err(invalid(
            "saas.insufficient_balance",
            "No eligible subscription or sufficient available balance",
        ));
    }
    let key_total =
        limits.spent_micros as i128 + limits.key_frozen as i128 + reserved_micros as i128;
    if limits
        .limit_micros
        .is_some_and(|limit| key_total > limit as i128)
    {
        return Err(invalid(
            "saas.key_quota",
            "API key spending limit has been reached",
        ));
    }
    let active: (i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(CASE WHEN key_id=? THEN 1 ELSE 0 END),0) FROM saas_billing_reservations WHERE user_id=? AND status='reserved'")
        .bind(&principal.key_id).bind(&principal.user_id).fetch_one(&mut *transaction).await.map_err(db_error)?;
    if active.0 >= limits.max_concurrency || active.1 >= limits.max_concurrency {
        return Err(invalid(
            "saas.concurrency_limit",
            "Too many concurrent requests",
        ));
    }
    let funding_type = if subscription_id.is_some() {
        "subscription"
    } else {
        "balance"
    };
    let user_frozen = repository::money(
        limits.user_frozen as i128
            + if subscription_id.is_none() {
                reserved_micros as i128
            } else {
                0
            },
    )?;
    let key_frozen = repository::money(limits.key_frozen as i128 + reserved_micros as i128)?;
    sqlx::query("UPDATE saas_users SET frozen_micros=?,updated_at=? WHERE id=?")
        .bind(user_frozen)
        .bind(repository::now())
        .bind(&principal.user_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    if let Some(subscription_id) = &subscription_id {
        sqlx::query("INSERT INTO saas_subscription_daily_usage(subscription_id,day,used_micros,frozen_micros,updated_at) VALUES(?,?,0,?,?) ON CONFLICT(subscription_id,day) DO UPDATE SET frozen_micros=frozen_micros+excluded.frozen_micros,updated_at=excluded.updated_at")
            .bind(subscription_id).bind(&subscription_day).bind(reserved_micros).bind(repository::now()).execute(&mut *transaction).await.map_err(db_error)?;
    }
    sqlx::query("UPDATE saas_api_keys SET frozen_micros=?,updated_at=? WHERE id=?")
        .bind(key_frozen)
        .bind(repository::now())
        .bind(&principal.key_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO saas_billing_reservations(request_id,user_id,key_id,group_id,platform,model,price_json,reserved_micros,status,funding_type,subscription_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'reserved',?,?,?,?)")
        .bind(&request_id).bind(&principal.user_id).bind(&principal.key_id).bind(&principal.group_id).bind(&principal.platform).bind(model)
        .bind(serde_json::to_string(&price).map_err(|_| invalid("saas.pricing", "Could not preserve pricing snapshot"))?).bind(reserved_micros).bind(funding_type).bind(&subscription_id).bind(repository::now()).bind(repository::now()).execute(&mut *transaction).await.map_err(db_error)?;
    transaction.commit().await.map_err(db_error)?;
    Ok(Reservation {
        request_id,
        user_id: principal.user_id.clone(),
        key_id: principal.key_id.clone(),
        group_id: principal.group_id.clone(),
        platform: principal.platform.clone(),
        model: model.into(),
        upstream_model,
        credential_ids,
        reserved_micros,
        price,
        max_output_tokens: limits.max_output_tokens,
        timeout_seconds: limits.timeout_seconds,
        max_concurrency: limits.max_concurrency,
    })
}

async fn reserve_fixed(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    principal: &ApiPrincipal,
    model: &str,
    upstream_model: String,
    credential_ids: Vec<String>,
    reserved_micros: i64,
    limits: Limits,
    price_json: String,
) -> Result<Reservation, AppError> {
    let subscription_day = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let subscription_id: Option<String> = if limits.allow_subscription {
        sqlx::query_scalar("SELECT subscriptions.id FROM saas_subscriptions subscriptions LEFT JOIN saas_subscription_daily_usage daily ON daily.subscription_id=subscriptions.id AND daily.day=? WHERE subscriptions.user_id=? AND subscriptions.status='active' AND subscriptions.starts_at<=? AND subscriptions.expires_at>? AND subscriptions.quota_micros-COALESCE(daily.used_micros,0)-COALESCE(daily.frozen_micros,0)>=? ORDER BY subscriptions.expires_at,subscriptions.created_at,subscriptions.id LIMIT 1")
            .bind(&subscription_day).bind(&principal.user_id).bind(repository::now()).bind(repository::now()).bind(reserved_micros).fetch_optional(&mut **transaction).await.map_err(db_error)?
    } else {
        None
    };
    if reserved_micros > 0
        && subscription_id.is_none()
        && (!limits.allow_balance || limits.balance_micros - limits.user_frozen < reserved_micros)
    {
        return Err(invalid(
            "saas.insufficient_balance",
            "No eligible subscription or sufficient available balance",
        ));
    }
    if limits.limit_micros.is_some_and(|limit| {
        limits.spent_micros as i128 + limits.key_frozen as i128 + reserved_micros as i128
            > limit as i128
    }) {
        return Err(invalid(
            "saas.key_quota",
            "API key spending limit has been reached",
        ));
    }
    let active: (i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(CASE WHEN key_id=? THEN 1 ELSE 0 END),0) FROM saas_billing_reservations WHERE user_id=? AND status='reserved'")
        .bind(&principal.key_id).bind(&principal.user_id).fetch_one(&mut **transaction).await.map_err(db_error)?;
    if active.0 >= limits.max_concurrency || active.1 >= limits.max_concurrency {
        return Err(invalid(
            "saas.concurrency_limit",
            "Too many concurrent requests",
        ));
    }
    let funding_type = if subscription_id.is_some() {
        "subscription"
    } else {
        "balance"
    };
    if subscription_id.is_none() {
        sqlx::query("UPDATE saas_users SET frozen_micros=frozen_micros+?,updated_at=? WHERE id=?")
            .bind(reserved_micros)
            .bind(repository::now())
            .bind(&principal.user_id)
            .execute(&mut **transaction)
            .await
            .map_err(db_error)?;
    } else if let Some(subscription_id) = &subscription_id {
        sqlx::query("INSERT INTO saas_subscription_daily_usage(subscription_id,day,used_micros,frozen_micros,updated_at) VALUES(?,?,0,?,?) ON CONFLICT(subscription_id,day) DO UPDATE SET frozen_micros=frozen_micros+excluded.frozen_micros,updated_at=excluded.updated_at")
            .bind(subscription_id).bind(&subscription_day).bind(reserved_micros).bind(repository::now()).execute(&mut **transaction).await.map_err(db_error)?;
    }
    sqlx::query("UPDATE saas_api_keys SET frozen_micros=frozen_micros+?,updated_at=? WHERE id=?")
        .bind(reserved_micros)
        .bind(repository::now())
        .bind(&principal.key_id)
        .execute(&mut **transaction)
        .await
        .map_err(db_error)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO saas_billing_reservations(request_id,user_id,key_id,group_id,platform,model,price_json,reserved_micros,status,funding_type,subscription_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'reserved',?,?,?,?)")
        .bind(&request_id).bind(&principal.user_id).bind(&principal.key_id).bind(&principal.group_id).bind(&principal.platform).bind(model).bind(price_json).bind(reserved_micros).bind(funding_type).bind(&subscription_id).bind(repository::now()).bind(repository::now()).execute(&mut **transaction).await.map_err(db_error)?;
    Ok(Reservation {
        request_id,
        user_id: principal.user_id.clone(),
        key_id: principal.key_id.clone(),
        group_id: principal.group_id.clone(),
        platform: principal.platform.clone(),
        model: model.into(),
        upstream_model,
        credential_ids,
        reserved_micros,
        price: ModelPrice {
            input_price_micros: 0,
            cache_price_micros: 0,
            output_price_micros: 0,
            multiplier_micros: 1_000_000,
        },
        max_output_tokens: limits.max_output_tokens,
        timeout_seconds: limits.timeout_seconds,
        max_concurrency: limits.max_concurrency,
    })
}

pub async fn settle_image(
    pool: &SqlitePool,
    request_id: &str,
    success_count: Option<i64>,
    success: bool,
) -> Result<Settlement, AppError> {
    let mut transaction = repository::begin(pool).await?;
    let mut reservation = load_reservation(&mut transaction, request_id).await?;
    if ["settled", "refunded"].contains(&reservation.status.as_str()) {
        return Ok(reservation.result());
    }
    if success_count.is_none() && success {
        sqlx::query("UPDATE saas_billing_reservations SET status='pending_review',updated_at=? WHERE request_id=?").bind(repository::now()).bind(request_id).execute(&mut *transaction).await.map_err(db_error)?;
        reservation.status = "pending_review".into();
        transaction.commit().await.map_err(db_error)?;
        return Ok(reservation.result());
    }
    let snapshot: serde_json::Value = serde_json::from_str(&reservation.price_json)
        .map_err(|_| invalid("saas.pricing_snapshot", "Invalid image price snapshot"))?;
    if snapshot.get("kind").and_then(serde_json::Value::as_str) != Some("image") {
        return Err(invalid(
            "saas.pricing_snapshot",
            "Reservation is not image billing",
        ));
    }
    let count = success_count.unwrap_or(0);
    let unit = snapshot
        .get("unitPriceMicros")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid("saas.pricing_snapshot", "Invalid image unit price"))?;
    let requested = snapshot
        .get("count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    if count < 0 || count > requested {
        return Err(invalid(
            "saas.pricing_snapshot",
            "Invalid successful image count",
        ));
    }
    let cost = repository::money(
        reservation.reserved_micros as i128 * count as i128 / requested.max(1) as i128,
    )?;
    let status = if count == 0 { "refunded" } else { "settled" };
    let result = finish(
        &mut transaction,
        &reservation,
        cost,
        None,
        status,
        "proxy",
        Some(&format!("image_count={count};unit={unit}")),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

async fn load_reservation(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<StoredReservation, AppError> {
    sqlx::query_as("SELECT request_id,user_id,key_id,group_id,model,price_json,reserved_micros,status,price_usd_micros,funding_type,subscription_id,created_at FROM saas_billing_reservations WHERE request_id=?")
        .bind(request_id).fetch_optional(connection).await.map_err(db_error)?.ok_or_else(|| invalid("saas.reservation_not_found", "Billing reservation is unavailable"))
}

pub async fn settle(
    pool: &SqlitePool,
    request_id: &str,
    usage: Option<BillableUsage>,
    success: bool,
) -> Result<Settlement, AppError> {
    let mut transaction = repository::begin(pool).await?;
    let mut reservation = load_reservation(&mut transaction, request_id).await?;
    if ["settled", "refunded"].contains(&reservation.status.as_str()) {
        return Ok(reservation.result());
    }
    if usage.is_none() && (success || reservation.status == "pending_review") {
        sqlx::query("UPDATE saas_billing_reservations SET status='pending_review',updated_at=? WHERE request_id=?").bind(repository::now()).bind(request_id).execute(&mut *transaction).await.map_err(db_error)?;
        reservation.status = "pending_review".into();
        transaction.commit().await.map_err(db_error)?;
        return Ok(reservation.result());
    }
    let price: ModelPrice = serde_json::from_str(&reservation.price_json).map_err(|_| {
        invalid(
            "saas.pricing_snapshot",
            "Stored pricing snapshot is invalid; manual review is required",
        )
    })?;
    let cost = match &usage {
        Some(usage) => price.charge(usage)?,
        None => 0,
    };
    let status = if usage.is_none() {
        "refunded"
    } else {
        "settled"
    };
    let result = finish(
        &mut transaction,
        &reservation,
        cost,
        usage.as_ref(),
        status,
        "proxy",
        None,
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

async fn finish(
    connection: &mut SqliteConnection,
    reservation: &StoredReservation,
    cost: i64,
    usage: Option<&BillableUsage>,
    status: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<Settlement, AppError> {
    let (balance, frozen): (i64, i64) =
        sqlx::query_as("SELECT balance_micros,frozen_micros FROM saas_users WHERE id=?")
            .bind(&reservation.user_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db_error)?;
    let (spent, key_frozen): (i64, i64) =
        sqlx::query_as("SELECT spent_micros,frozen_micros FROM saas_api_keys WHERE id=?")
            .bind(&reservation.key_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db_error)?;
    if cost < 0
        || (reservation.funding_type == "balance" && frozen < reservation.reserved_micros)
        || key_frozen < reservation.reserved_micros
    {
        return Err(invalid(
            "saas.billing_invariant",
            "Billing balances require manual review",
        ));
    }
    let after = repository::money(
        balance as i128
            - if reservation.funding_type == "balance" {
                cost as i128
            } else {
                0
            },
    )?;
    let total_spent = repository::money(spent as i128 + cost as i128)?;
    let current = repository::now();
    sqlx::query("UPDATE saas_users SET balance_micros=?,frozen_micros=frozen_micros-?,updated_at=? WHERE id=?").bind(after).bind(if reservation.funding_type == "balance" { reservation.reserved_micros } else { 0 }).bind(current).bind(&reservation.user_id).execute(&mut *connection).await.map_err(db_error)?;
    sqlx::query("UPDATE saas_api_keys SET spent_micros=?,frozen_micros=frozen_micros-?,updated_at=? WHERE id=?").bind(total_spent).bind(reservation.reserved_micros).bind(current).bind(&reservation.key_id).execute(&mut *connection).await.map_err(db_error)?;
    if let Some(subscription_id) = &reservation.subscription_id {
        let day = repository::timestamp(reservation.created_at)[..10].to_string();
        let available: Option<i64> = sqlx::query_scalar("SELECT subscriptions.quota_micros-daily.used_micros FROM saas_subscriptions subscriptions JOIN saas_subscription_daily_usage daily ON daily.subscription_id=subscriptions.id AND daily.day=? WHERE subscriptions.id=? AND daily.frozen_micros>=?")
            .bind(&day).bind(subscription_id).bind(reservation.reserved_micros).fetch_optional(&mut *connection).await.map_err(db_error)?;
        if available.is_none_or(|value| value < cost) {
            return Err(invalid(
                "saas.billing_invariant",
                "Subscription daily quota requires manual review",
            ));
        }
        sqlx::query("UPDATE saas_subscription_daily_usage SET used_micros=used_micros+?,frozen_micros=frozen_micros-?,updated_at=? WHERE subscription_id=? AND day=?")
            .bind(cost).bind(reservation.reserved_micros).bind(current).bind(subscription_id).bind(&day).execute(&mut *connection).await.map_err(db_error)?;
        let used_after: i64 = sqlx::query_scalar("SELECT used_micros FROM saas_subscription_daily_usage WHERE subscription_id=? AND day=?").bind(subscription_id).bind(&day).fetch_one(&mut *connection).await.map_err(db_error)?;
        sqlx::query("INSERT INTO saas_subscription_ledger(id,subscription_id,user_id,kind,amount_micros,used_after_micros,request_id,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(subscription_id).bind(&reservation.user_id).bind(if status=="refunded"{"release"}else{"usage"}).bind(cost).bind(used_after).bind(&reservation.request_id).bind(reason).bind(current).execute(&mut *connection).await.map_err(db_error)?;
    } else {
        sqlx::query("INSERT INTO saas_wallet_ledger(id,user_id,kind,amount_micros,balance_after_micros,source_id,request_id,idempotency_key,reason,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&reservation.user_id).bind(if status=="refunded" {"release"} else {"usage"}).bind(-cost).bind(after).bind(&reservation.request_id).bind(&reservation.request_id)
        .bind(format!("billing:{}",reservation.request_id)).bind(reason).bind(actor).bind(current).execute(&mut *connection).await.map_err(db_error)?;
    }
    if status == "settled" {
        let unconfirmed = i64::from(usage.is_none());
        let empty_usage = BillableUsage::default();
        let usage = usage.unwrap_or(&empty_usage);
        sqlx::query("INSERT INTO saas_usage_hourly(user_id,key_id,group_id,model,hour,request_count,input_tokens,cache_read_tokens,cache_write_tokens,output_tokens,cost_micros,unconfirmed_usage_count) VALUES(?,?,?,?,?,1,?,?,?,?,?,?) ON CONFLICT(user_id,key_id,group_id,model,hour) DO UPDATE SET request_count=request_count+1,input_tokens=input_tokens+excluded.input_tokens,cache_read_tokens=cache_read_tokens+excluded.cache_read_tokens,cache_write_tokens=cache_write_tokens+excluded.cache_write_tokens,output_tokens=output_tokens+excluded.output_tokens,cost_micros=cost_micros+excluded.cost_micros,unconfirmed_usage_count=unconfirmed_usage_count+excluded.unconfirmed_usage_count")
            .bind(&reservation.user_id).bind(&reservation.key_id).bind(&reservation.group_id).bind(&reservation.model).bind(reservation.created_at/3600*3600).bind(usage.input_tokens).bind(usage.cache_read_tokens).bind(usage.cache_write_tokens).bind(usage.output_tokens).bind(cost)
            .bind(unconfirmed)
            .execute(&mut *connection).await.map_err(db_error)?;
    }
    sqlx::query("UPDATE saas_billing_reservations SET status=?,price_usd_micros=?,updated_at=? WHERE request_id=? AND status IN ('reserved','pending_review')")
        .bind(status).bind(cost).bind(current).bind(&reservation.request_id).execute(&mut *connection).await.map_err(db_error)?;
    let mut result = reservation.result();
    result.status = status.into();
    result.price_usd_micros = Some(cost);
    Ok(result)
}

pub async fn recover_pending(pool: &SqlitePool) -> Result<u64, AppError> {
    let mut transaction = repository::begin(pool).await?;
    let count = sqlx::query("UPDATE saas_billing_reservations SET status='pending_review',updated_at=? WHERE status='reserved'")
        .bind(repository::now()).execute(&mut *transaction).await.map_err(db_error)?.rows_affected();
    if count > 0 {
        repository::audit(
            &mut transaction,
            "ledger.recover",
            None,
            json!({"pendingRequests":count}),
        )
        .await?;
    }
    transaction.commit().await.map_err(db_error)?;
    Ok(count)
}

pub async fn reconcile(
    pool: &SqlitePool,
    payload: serde_json::Value,
) -> Result<Settlement, AppError> {
    let request_id = repository::text(&payload, "requestId")?;
    let action = repository::text(&payload, "action")?;
    let reason = repository::text(&payload, "reason")?;
    if !["refund", "settle"].contains(&action) {
        return Err(invalid(
            "saas.validation",
            "Reconciliation action must be refund or settle",
        ));
    }
    let mut transaction = repository::begin(pool).await?;
    let reservation = load_reservation(&mut transaction, request_id).await?;
    if ["settled", "refunded"].contains(&reservation.status.as_str()) {
        return Ok(reservation.result());
    }
    if reservation.status != "pending_review" {
        return Err(invalid(
            "saas.billing_active",
            "Only requests awaiting review may be reconciled",
        ));
    }
    let usage: Option<BillableUsage> = payload
        .get("usage")
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|_| {
                invalid(
                    "saas.validation",
                    "All four usage categories must be provided",
                )
            })
        })
        .transpose()?;
    let cost = if action == "refund" {
        0
    } else if let Some(usage) = &usage {
        let price: ModelPrice = serde_json::from_str(&reservation.price_json)
            .map_err(|_| invalid("saas.pricing_snapshot", "Invalid price snapshot"))?;
        price.charge(usage)?
    } else {
        payload
            .get("amountMicros")
            .and_then(serde_json::Value::as_i64)
            .filter(|amount| (0..=repository::MAX_MONEY).contains(amount))
            .ok_or_else(|| {
                invalid(
                    "saas.validation",
                    "Verified usage or amountMicros is required",
                )
            })?
    };
    let result = finish(
        &mut transaction,
        &reservation,
        cost,
        if action == "refund" {
            None
        } else {
            usage.as_ref()
        },
        if action == "refund" {
            "refunded"
        } else {
            "settled"
        },
        "administrator",
        Some(reason),
    )
    .await?;
    repository::audit(
        &mut transaction,
        "ledger.reconcile",
        Some(request_id),
        json!({"action":action,"reason":reason,"amountMicros":cost}),
    )
    .await?;
    transaction.commit().await.map_err(db_error)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
