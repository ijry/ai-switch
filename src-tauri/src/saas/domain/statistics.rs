use crate::error::AppError;
use crate::saas::repository::{self, db_error, invalid};
use chrono::{Days, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

const FIELDS: [&str; 10] = [
    "rechargeCnyFen",
    "rechargeCreditMicros",
    "manualCreditMicros",
    "rewardMicros",
    "subscriptionSalesMicros",
    "walletUsageMicros",
    "subscriptionUsageMicros",
    "requestCount",
    "newUsers",
    "quotaMicros",
];

fn date(payload: &Value, key: &str, default: NaiveDate) -> Result<NaiveDate, AppError> {
    match payload.get(key) {
        None => Ok(default),
        Some(Value::String(value)) if value.len() == 10 => {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| invalid("saas.date", "Invalid date"))
        }
        _ => Err(invalid("saas.date", "Expected YYYY-MM-DD")),
    }
}
fn empty(period: &str) -> Value {
    let mut row = json!({"period":period});
    for field in FIELDS {
        row[field] = json!(0);
    }
    row
}
fn increment(row: &mut Value, field: &str, amount: i64) -> Result<(), AppError> {
    let next = row[field]
        .as_i64()
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or_else(|| {
            invalid(
                "saas.statistics_overflow",
                "Statistics exceed supported range",
            )
        })?;
    row[field] = json!(next);
    Ok(())
}

pub async fn report(pool: &SqlitePool, payload: Value) -> Result<Value, AppError> {
    let today = Utc::now().date_naive();
    let from = date(&payload, "from", today - Days::new(29))?;
    let to = date(&payload, "to", today)?;
    let granularity = payload
        .get("granularity")
        .and_then(Value::as_str)
        .unwrap_or("day");
    if !["day", "month"].contains(&granularity)
        || from > to
        || to > today
        || (to - from).num_days() > 3660
    {
        return Err(invalid(
            "saas.date",
            "Select a past or current date range up to ten years, grouped by day or month",
        ));
    }
    let start = from.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    let end = to
        .succ_opt()
        .ok_or_else(|| invalid("saas.date", "Invalid end date"))?
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();
    let mut tx = pool.begin().await.map_err(db_error)?;
    let mut days = BTreeMap::new();
    for offset in 0..=(to - from).num_days() {
        let day = (from + Days::new(offset as u64)).to_string();
        days.insert(day.clone(), empty(&day));
    }
    let events: Vec<(String, String, i64)> = sqlx::query_as(
        "WITH events AS (
        SELECT updated_at AS time, 'rechargeCnyFen' AS field, amount_cny_fen AS amount FROM saas_recharge_orders WHERE status='approved'
        UNION ALL SELECT updated_at,'rechargeCreditMicros',credit_micros FROM saas_recharge_orders WHERE status='approved'
        UNION ALL SELECT created_at,CASE kind WHEN 'usage' THEN 'walletUsageMicros' WHEN 'subscription_purchase' THEN 'subscriptionSalesMicros' WHEN 'recharge' THEN 'manualCreditMicros' ELSE 'rewardMicros' END,CASE WHEN kind IN ('usage','subscription_purchase') THEN -amount_micros ELSE amount_micros END FROM saas_wallet_ledger WHERE kind IN ('usage','subscription_purchase','checkin','invite_reward','redeem') OR (kind='recharge' AND NOT EXISTS(SELECT 1 FROM saas_recharge_orders orders WHERE orders.id=saas_wallet_ledger.source_id))
        UNION ALL SELECT created_at,'subscriptionUsageMicros',amount_micros FROM saas_subscription_ledger WHERE kind='usage'
        UNION ALL SELECT updated_at,'requestCount',1 FROM saas_billing_reservations WHERE status IN ('settled','refunded')
        UNION ALL SELECT created_at,'newUsers',1 FROM saas_users)
        SELECT strftime('%Y-%m-%d',time,'unixepoch'),field,SUM(amount) FROM events WHERE time>=? AND time<? GROUP BY 1,2")
        .bind(start).bind(end).fetch_all(&mut *tx).await.map_err(db_error)?;
    for (day, field, amount) in events {
        if let Some(row) = days.get_mut(&day) {
            increment(row, &field, amount)?;
        }
    }
    let subscriptions: Vec<(i64,i64,i64)> = sqlx::query_as("SELECT starts_at,CASE WHEN status='cancelled' THEN MIN(expires_at,updated_at) ELSE expires_at END,quota_micros FROM saas_subscriptions WHERE starts_at<? AND expires_at>?")
        .bind(end).bind(start).fetch_all(&mut *tx).await.map_err(db_error)?;
    for (day, row) in days.iter_mut() {
        let boundary = NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        for (starts, expires, quota) in &subscriptions {
            if *starts < boundary + 86400 && *expires > boundary {
                increment(row, "quotaMicros", *quota)?;
            }
        }
    }
    let now = repository::now();
    let current: (i64,i64,i64,i64) = sqlx::query_as("SELECT COUNT(*),COUNT(DISTINCT user_id),COALESCE(SUM(quota_micros),0),COALESCE(SUM(CASE WHEN expires_at<=? THEN 1 ELSE 0 END),0) FROM saas_subscriptions WHERE status='active' AND starts_at<=? AND expires_at>?")
        .bind(now+7*86400).bind(now).bind(now).fetch_one(&mut *tx).await.map_err(db_error)?;
    let today_used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(used_micros),0) FROM saas_subscription_daily_usage WHERE day=?",
    )
    .bind(today.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    let today_quota: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quota_micros),0) FROM saas_subscriptions WHERE starts_at<? AND CASE WHEN status='cancelled' THEN MIN(expires_at,updated_at) ELSE expires_at END>?")
        .bind(today.and_hms_opt(0,0,0).unwrap().and_utc().timestamp()+86400).bind(today.and_hms_opt(0,0,0).unwrap().and_utc().timestamp()).fetch_one(&mut *tx).await.map_err(db_error)?;
    let pending_rewards: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM saas_invite_rewards WHERE status='pending'")
            .fetch_one(&mut *tx)
            .await
            .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    let mut totals = empty("total");
    let mut buckets = BTreeMap::new();
    for (day, row) in days {
        let period = if granularity == "month" {
            &day[..7]
        } else {
            &day
        };
        let bucket = buckets
            .entry(period.to_owned())
            .or_insert_with(|| empty(period));
        for field in FIELDS {
            let amount = row[field].as_i64().unwrap_or(0);
            increment(bucket, field, amount)?;
            increment(&mut totals, field, amount)?;
        }
    }
    Ok(
        json!({"from":from.to_string(),"to":to.to_string(),"granularity":granularity,"timezone":"UTC","items":buckets.into_values().collect::<Vec<_>>(),"totals":totals,"current":{"activeSubscriptions":current.0,"subscribedUsers":current.1,"dailyQuotaMicros":current.2,"expiringSubscriptions":current.3,"todayUsedMicros":today_used,"todayQuotaMicros":today_quota,"pendingRewards":pending_rewards}}),
    )
}
