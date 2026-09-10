use super::*;
use crate::saas::{domain, repository};
use serde_json::json;

#[tokio::test]
async fn statistics_includes_wallet_and_subscription_settlements_after_cancellation() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 1000000).await;
    let reserved = reserve(&pool, &principal, "gpt-test", 2, 2).await.unwrap();
    settle(
        &pool,
        &reserved.request_id,
        Some(BillableUsage {
            input_tokens: 2,
            output_tokens: 2,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    let plan = domain::admin(&pool,"subscriptions.plans.save",json!({"name":"Plan","kind":"month","durationDays":30,"quotaMicros":1000000,"priceMicros":0})).await.unwrap();
    let subscription = domain::admin(
        &pool,
        "subscriptions.grant",
        json!({"userId":principal.user_id,"planId":plan["id"]}),
    )
    .await
    .unwrap();
    let reserved = reserve(&pool, &principal, "gpt-test", 2, 2).await.unwrap();
    domain::admin(
        &pool,
        "subscriptions.cancel",
        json!({"userId":principal.user_id,"id":subscription["id"],"reason":"Stop new requests"}),
    )
    .await
    .unwrap();
    settle(
        &pool,
        &reserved.request_id,
        Some(BillableUsage {
            input_tokens: 2,
            output_tokens: 2,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    let report = domain::admin(&pool, "statistics", json!({})).await.unwrap();
    assert_eq!(report["totals"]["walletUsageMicros"], 6);
    assert_eq!(report["totals"]["subscriptionUsageMicros"], 6);
    assert_eq!(report["totals"]["requestCount"], 2);
    assert_eq!(report["current"]["activeSubscriptions"], 0);
}

#[tokio::test]
async fn administrator_can_credit_a_user_with_ledger_and_audit_records() {
    let pool = repository::test_pool().await;
    let (user_id, _) = repository::test_user_group(&pool).await;
    let result = domain::admin(
        &pool,
        "users.credit",
        json!({"userId":user_id,"amountMicros":2_500_000,"reason":"Manual service credit"}),
    )
    .await
    .unwrap();
    assert_eq!(result["balanceMicros"], 2_500_000);
    let ledger: (i64, String, String, String) = sqlx::query_as(
        "SELECT amount_micros,kind,actor,reason FROM saas_wallet_ledger WHERE user_id=?",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ledger,
        (
            2_500_000,
            "recharge".into(),
            "administrator".into(),
            "Manual service credit".into()
        )
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM saas_admin_audit WHERE operation='users.credit' AND target_id=?",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    assert!(domain::admin(
        &pool,
        "users.credit",
        json!({"userId":user_id,"amountMicros":0,"reason":"Invalid"}),
    )
    .await
    .is_err());
}

#[test]
fn pricing_uses_wide_integers_and_rounds_only_the_final_microdollar() {
    let price = ModelPrice {
        input_price_micros: 1_000_000,
        cache_price_micros: 100_000,
        output_price_micros: 2_000_000,
        multiplier_micros: 1_500_000,
    };
    assert_eq!(
        price
            .charge(&BillableUsage {
                input_tokens: 1,
                cache_read_tokens: 1,
                cache_write_tokens: 1,
                output_tokens: 1
            })
            .unwrap(),
        7
    );
    assert_eq!(price.charge(&BillableUsage::default()).unwrap(), 0);
    assert!(price
        .charge(&BillableUsage {
            input_tokens: -1,
            ..Default::default()
        })
        .is_err());
    assert!(price
        .charge(&BillableUsage {
            input_tokens: i64::MAX,
            ..Default::default()
        })
        .is_err());
    assert_eq!(pricing::recharge_credit(100, 7_000_000).unwrap(), 142_857);
}

#[tokio::test]
async fn reservation_blocks_double_spend_and_settlement_is_idempotent() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 20).await;
    let reservation = reserve(&pool, &principal, "gpt-test", 10, 5).await.unwrap();
    assert_eq!(reservation.reserved_micros, 20);
    assert!(reserve(&pool, &principal, "gpt-test", 1, 1).await.is_err());
    assert!(reserve(&pool, &principal, "account-default/gpt-test", 1, 1)
        .await
        .is_err());
    let usage = BillableUsage {
        input_tokens: 2,
        output_tokens: 2,
        ..Default::default()
    };
    let first = settle(&pool, &reservation.request_id, Some(usage), true)
        .await
        .unwrap();
    let second = settle(
        &pool,
        &reservation.request_id,
        Some(BillableUsage {
            input_tokens: 100,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    assert_eq!(first.price_usd_micros, Some(6));
    assert_eq!(second.price_usd_micros, Some(6));
    let balance: (i64, i64) =
        sqlx::query_as("SELECT balance_micros,frozen_micros FROM saas_users WHERE id=?")
            .bind(&principal.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(balance, (14, 0));
    let ledger: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM saas_wallet_ledger WHERE request_id=?")
            .bind(&reservation.request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ledger, 1);
}

#[tokio::test]
async fn missing_usage_remains_frozen_and_manual_reconciliation_is_final() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 100).await;
    let reserved = reserve(&pool, &principal, "gpt-test", 10, 5).await.unwrap();
    let pending = settle(&pool, &reserved.request_id, None, true)
        .await
        .unwrap();
    assert_eq!(pending.status, "pending_review");
    assert_eq!(pending.price_usd_micros, None);
    let frozen: i64 = sqlx::query_scalar("SELECT frozen_micros FROM saas_users WHERE id=?")
        .bind(&principal.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(frozen, 20);
    let result = domain::admin(&pool, "ledger.reconcile", json!({"requestId":reserved.request_id,"action":"refund","reason":"Verified upstream rejected request"})).await.unwrap();
    assert_eq!(result["status"], "refunded");
    assert_eq!(
        settle(
            &pool,
            &reserved.request_id,
            Some(BillableUsage {
                input_tokens: 5,
                ..Default::default()
            }),
            true
        )
        .await
        .unwrap()
        .price_usd_micros,
        Some(0)
    );
}

#[tokio::test]
async fn real_overage_creates_debt_instead_of_free_usage() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 20).await;
    let reserved = reserve(&pool, &principal, "gpt-test", 1, 1).await.unwrap();
    settle(
        &pool,
        &reserved.request_id,
        Some(BillableUsage {
            input_tokens: 30,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(&principal.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, -10);
    assert!(reserve(&pool, &principal, "gpt-test", 1, 1).await.is_err());
}

#[tokio::test]
async fn recharge_approval_and_concurrent_redemption_credit_exactly_once() {
    let pool = repository::test_pool().await;
    let (user_id, _) = repository::test_user_group(&pool).await;
    let order = domain::user(
        &pool,
        &user_id,
        "recharges.create",
        json!({"amountCnyFen":100,"requestId":"order-once"}),
    )
    .await
    .unwrap();
    crate::saas::config::save(&pool, json!({"exchangeRateMicros":8_000_000}))
        .await
        .unwrap();
    let payload = json!({"id":order["id"],"status":"approved","reason":"Received cash"});
    let (first, second) = tokio::join!(
        domain::admin(&pool, "recharges.review", payload.clone()),
        domain::admin(&pool, "recharges.review", payload)
    );
    assert!(first.is_ok() && second.is_ok());
    let batch = domain::admin(
        &pool,
        "codes.create",
        json!({"count":1,"amountMicros":1_000_000}),
    )
    .await
    .unwrap();
    let redeem = json!({"code":batch["items"][0]["plaintextCode"]});
    let (first, second) = tokio::join!(
        domain::user(&pool, &user_id, "redeem", redeem.clone()),
        domain::user(&pool, &user_id, "redeem", redeem)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, 1_142_857);
}

#[tokio::test]
async fn unknown_usage_cannot_be_refunded_by_a_late_unconfirmed_callback() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 100).await;
    let reserved = reserve(&pool, &principal, "gpt-test", 10, 5).await.unwrap();
    settle(&pool, &reserved.request_id, None, true)
        .await
        .unwrap();
    let repeated = settle(&pool, &reserved.request_id, None, false)
        .await
        .unwrap();
    assert_eq!(repeated.status, "pending_review");
    assert_eq!(repeated.price_usd_micros, None);
}

#[tokio::test]
async fn manual_amount_reconciliation_is_included_in_hourly_cost_without_fabricated_tokens() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 100).await;
    let reserved = reserve(&pool, &principal, "gpt-test", 10, 5).await.unwrap();
    settle(&pool, &reserved.request_id, None, true)
        .await
        .unwrap();
    domain::admin(&pool,"ledger.reconcile",json!({"requestId":reserved.request_id,"action":"settle","amountMicros":13,"reason":"Provider verified billed amount, token counts unavailable"})).await.unwrap();
    let hourly: (i64,i64,i64) = sqlx::query_as("SELECT cost_micros,request_count,unconfirmed_usage_count FROM saas_usage_hourly WHERE user_id=?").bind(&principal.user_id).fetch_one(&pool).await.unwrap();
    assert_eq!(hourly, (13, 1, 1));
    let overview = domain::user(&pool, &principal.user_id, "overview", json!({}))
        .await
        .unwrap();
    assert_eq!(overview["today"]["costMicros"], 13);
}

#[tokio::test]
async fn multi_connection_reservations_cannot_overspend_and_recovery_keeps_funds_frozen() {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    let temporary = tempfile::TempDir::new().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(temporary.path().join("saas.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .unwrap();
    repository::migrate(&pool).await.unwrap();
    let principal = repository::test_principal(&pool, 20).await;
    let (first, second) = tokio::join!(
        reserve(&pool, &principal, "gpt-test", 10, 5),
        reserve(&pool, &principal, "gpt-test", 10, 5)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(recover_pending(&pool).await.unwrap(), 1);
    assert_eq!(recover_pending(&pool).await.unwrap(), 0);
    let frozen: i64 = sqlx::query_scalar("SELECT frozen_micros FROM saas_users WHERE id=?")
        .bind(&principal.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(frozen, 20);
    pool.close().await;
}

#[tokio::test]
async fn key_quota_prices_and_user_status_are_rechecked_but_received_requests_keep_their_snapshot()
{
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 100).await;
    domain::user(
        &pool,
        &principal.user_id,
        "keys.update",
        json!({"id":principal.key_id,"limitMicros":10}),
    )
    .await
    .unwrap();
    assert!(reserve(&pool, &principal, "gpt-test", 10, 5).await.is_err());
    let reserved = reserve(&pool, &principal, "gpt-test", 2, 2).await.unwrap();
    let mut group = repository::test_group_payload();
    group["id"] = json!(principal.group_id);
    group["models"][0]["inputPriceMicros"] = json!(20_000_000);
    domain::admin(&pool, "groups.save", group).await.unwrap();
    domain::admin(
        &pool,
        "users.status",
        json!({"id":principal.user_id,"status":"banned"}),
    )
    .await
    .unwrap();
    assert!(reserve(&pool, &principal, "gpt-test", 1, 1).await.is_err());
    let result = settle(
        &pool,
        &reserved.request_id,
        Some(BillableUsage {
            input_tokens: 2,
            output_tokens: 2,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    assert_eq!(result.price_usd_micros, Some(6));
    let usage_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usage_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage_events, 0);
}

#[tokio::test]
async fn recharge_idempotency_ownership_and_review_cancel_race_preserve_money() {
    let pool = repository::test_pool().await;
    let (user_id, _) = repository::test_user_group(&pool).await;
    let (other_user, _) = repository::test_user_group(&pool).await;
    let payload = json!({"requestId":"fixed-order","amountCnyFen":100});
    let order = domain::user(&pool, &user_id, "recharges.create", payload.clone())
        .await
        .unwrap();
    assert_eq!(
        domain::user(&pool, &user_id, "recharges.create", payload)
            .await
            .unwrap()["id"],
        order["id"]
    );
    assert!(domain::user(
        &pool,
        &user_id,
        "recharges.create",
        json!({"requestId":"fixed-order","amountCnyFen":200})
    )
    .await
    .is_err());
    assert!(domain::user(
        &pool,
        &other_user,
        "recharges.cancel",
        json!({"id":order["id"]})
    )
    .await
    .is_err());
    let listed = domain::user(
        &pool,
        &other_user,
        "recharges.list",
        json!({"userId":user_id}),
    )
    .await
    .unwrap();
    assert_eq!(listed["total"], 0);
    let (cancel, review) = tokio::join!(
        domain::user(
            &pool,
            &user_id,
            "recharges.cancel",
            json!({"id":order["id"]})
        ),
        domain::admin(
            &pool,
            "recharges.review",
            json!({"id":order["id"],"status":"approved","reason":"Cash received"})
        )
    );
    assert_eq!(usize::from(cancel.is_ok()) + usize::from(review.is_ok()), 1);
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, if review.is_ok() { 142_857 } else { 0 });
}
#[tokio::test]
async fn dashboards_read_core_groups_and_usage_totals_span_all_pages() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 1000).await;
    let admin = domain::admin(&pool, "overview", json!({})).await.unwrap();
    assert!(admin["groupCount"].as_i64().unwrap() >= 7);
    for days in [0, 1] {
        let reservation = reserve(&pool, &principal, "gpt-test", 2, 2).await.unwrap();
        sqlx::query(
            "UPDATE saas_billing_reservations SET created_at=created_at-? WHERE request_id=?",
        )
        .bind(days * 86400)
        .bind(&reservation.request_id)
        .execute(&pool)
        .await
        .unwrap();
        settle(
            &pool,
            &reservation.request_id,
            Some(BillableUsage {
                input_tokens: 2,
                output_tokens: 2,
                ..Default::default()
            }),
            true,
        )
        .await
        .unwrap();
    }
    let result = domain::user(
        &pool,
        &principal.user_id,
        "usage",
        json!({"pageSize":1,"keyId":"","model":""}),
    )
    .await
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["total"], 2);
    assert_eq!(result["totals"]["requestCount"], 2);
    assert_eq!(result["totals"]["costMicros"], 12);
    assert_eq!(result["buckets"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn daily_subscription_quota_resets_each_utc_day_and_falls_back_to_balance() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 1_008_192).await;
    let plan = domain::admin(
        &pool,
        "subscriptions.plans.save",
        json!({
            "name":"Daily test plan",
            "kind":"day",
            "durationDays":1,
            "quotaMicros":1_008_192,
            "priceMicros":0
        }),
    )
    .await
    .unwrap();
    let subscription = domain::admin(
        &pool,
        "subscriptions.grant",
        json!({"userId":principal.user_id,"planId":plan["id"]}),
    )
    .await
    .unwrap();

    let first = reserve(&pool, &principal, "gpt-test", 1_000_000, 4_096)
        .await
        .unwrap();
    let funding: String =
        sqlx::query_scalar("SELECT funding_type FROM saas_billing_reservations WHERE request_id=?")
            .bind(&first.request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(funding, "subscription");
    settle(
        &pool,
        &first.request_id,
        Some(BillableUsage {
            input_tokens: 1_000_000,
            output_tokens: 4_096,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();

    let second = reserve(&pool, &principal, "gpt-test", 1_000_000, 4_096)
        .await
        .unwrap();
    let funding: String =
        sqlx::query_scalar("SELECT funding_type FROM saas_billing_reservations WHERE request_id=?")
            .bind(&second.request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(funding, "balance");
    settle(
        &pool,
        &second.request_id,
        Some(BillableUsage {
            input_tokens: 1_000_000,
            output_tokens: 4_096,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(&principal.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, 0);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    sqlx::query("UPDATE saas_subscription_daily_usage SET day='2000-01-01' WHERE subscription_id=? AND day=?")
        .bind(subscription["id"].as_str().unwrap())
        .bind(&today)
        .execute(&pool)
        .await
        .unwrap();
    let third = reserve(&pool, &principal, "gpt-test", 1_000_000, 4_096)
        .await
        .unwrap();
    let funding: String =
        sqlx::query_scalar("SELECT funding_type FROM saas_billing_reservations WHERE request_id=?")
            .bind(&third.request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(funding, "subscription");
    settle(
        &pool,
        &third.request_id,
        Some(BillableUsage {
            input_tokens: 1_000_000,
            output_tokens: 4_096,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    let used: i64 = sqlx::query_scalar(
        "SELECT used_micros FROM saas_subscription_daily_usage WHERE subscription_id=? AND day=?",
    )
    .bind(subscription["id"].as_str().unwrap())
    .bind(&today)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(used, 1_008_192);
    let balance: i64 = sqlx::query_scalar("SELECT balance_micros FROM saas_users WHERE id=?")
        .bind(&principal.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(balance, 0);
}

#[tokio::test]
async fn image_billing_reserves_per_image_and_refunds_failed_units() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 1_000_000).await;
    sqlx::query(r#"UPDATE route_credentials SET config_json=json_set(config_json,'$.model_mappings',json('[{"from":"gpt-test","to":"gpt-image-2.5","capabilities":["image.generate"]}]')) WHERE id IN ('account-default','account-custom')"#)
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE saas_group_models SET upstream_model='gpt-test',image_price_micros=25000 WHERE group_id=? AND model='gpt-test'")
        .bind(&principal.group_id).execute(&pool).await.unwrap();
    let reservation = super::reserve_image(&pool, &principal, "gpt-test", 3)
        .await
        .unwrap();
    assert_eq!(reservation.reserved_micros, 75_000);
    let settlement = super::settle_image(&pool, &reservation.request_id, Some(2), true)
        .await
        .unwrap();
    assert_eq!(settlement.status, "settled");
    assert_eq!(settlement.price_usd_micros, Some(50_000));
    let user: (i64, i64) =
        sqlx::query_as("SELECT balance_micros,frozen_micros FROM saas_users WHERE id=?")
            .bind(&principal.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user, (950_000, 0));
}
