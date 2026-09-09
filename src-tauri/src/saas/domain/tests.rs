use super::*;
use crate::saas::repository;
use serde_json::json;

#[tokio::test]
async fn admin_lists_only_configured_groups_and_offers_unconfigured_groups_for_addition() {
    let pool = repository::test_pool().await;
    repository::test_host(&pool).await;
    let before = admin(&pool, "groups.list", json!({})).await.unwrap();
    assert!(before["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|group| group["id"] != "saas-test-group"));
    let available = admin(&pool, "groups.available", json!({})).await.unwrap();
    assert!(available["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|group| group["id"] == "saas-test-group" && group["configured"] == false));

    admin(&pool, "groups.save", repository::test_group_payload())
        .await
        .unwrap();
    let after = admin(&pool, "groups.list", json!({})).await.unwrap();
    assert!(after["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|group| group["id"] == "saas-test-group" && group["configured"] == true));
    let available = admin(&pool, "groups.available", json!({})).await.unwrap();
    assert!(available["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|group| group["id"] != "saas-test-group"));
}

#[tokio::test]
async fn groups_require_explicit_prices_and_empty_scope_never_means_all() {
    let pool = repository::test_pool().await;
    repository::test_host(&pool).await;
    let mut group = repository::test_group_payload();
    group["models"][0]
        .as_object_mut()
        .unwrap()
        .remove("cachePriceMicros");
    assert!(admin(&pool, "groups.save", group).await.is_err());
    let saved = admin(&pool, "groups.save", repository::test_group_payload())
        .await
        .unwrap();
    let group_id = saved["id"].as_str().unwrap();
    assert_eq!(
        permitted_accounts(&pool, group_id).await.unwrap(),
        vec!["account-custom", "account-default"]
    );
    sqlx::query("UPDATE route_pool_groups SET name='renamed' WHERE id=?")
        .bind(group_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        permitted_accounts(&pool, group_id).await.unwrap(),
        vec!["account-custom", "account-default"]
    );
    sqlx::query("UPDATE route_pool_members SET group_id='codex-default' WHERE group_id=?")
        .bind(group_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(permitted_accounts(&pool, group_id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn extensions_cannot_create_or_delete_core_groups_and_internal_keys_fail_closed() {
    let pool = repository::test_pool().await;
    let (user_id, group_id) = repository::test_user_group(&pool).await;
    let key = user(
        &pool,
        &user_id,
        "keys.create",
        json!({"groupId":group_id,"name":"public"}),
    )
    .await
    .unwrap();
    let plain = key["plaintextKey"].as_str().unwrap();
    let active: bool = sqlx::query_scalar("SELECT is_active FROM route_pool_groups WHERE id=?")
        .bind(&group_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!active);
    assert!(crate::saas::billing::authenticate_key(&pool, plain)
        .await
        .is_ok());
    let mut payload = repository::test_group_payload();
    payload["id"] = json!("missing-core-group");
    assert!(admin(&pool, "groups.save", payload).await.is_err());
    assert!(admin(&pool, "groups.delete", json!({"id":group_id}))
        .await
        .is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM route_pool_groups WHERE id='missing-core-group'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    sqlx::query("UPDATE route_pool_groups SET is_internal=1 WHERE id=?")
        .bind(&group_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(crate::saas::billing::authenticate_key(&pool, plain)
        .await
        .is_err());
    assert!(permitted_accounts(&pool, &group_id)
        .await
        .unwrap()
        .is_empty());
    assert!(user(
        &pool,
        &user_id,
        "keys.create",
        json!({"groupId":group_id,"name":"internal"})
    )
    .await
    .is_err());
    let listed = user(&pool, &user_id, "groups", json!({})).await.unwrap();
    assert!(listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|group| group["id"] != group_id));
    sqlx::query("UPDATE route_pool_groups SET is_internal=0,name='renamed' WHERE id=?")
        .bind(&group_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(crate::saas::billing::authenticate_key(&pool, plain)
        .await
        .is_ok());
}

#[tokio::test]
async fn mapped_models_use_only_bound_group_accounts_and_preserve_prices() {
    let pool = repository::test_pool().await;
    let principal = repository::test_principal(&pool, 100).await;
    let mut payload = repository::test_group_payload();
    payload["models"][0]["model"] = json!("public-model");
    payload["models"][0]["upstreamModel"] = json!("gpt-test");
    let saved = admin(&pool, "groups.save", payload).await.unwrap();
    assert_eq!(saved["models"][0]["upstreamModel"], "gpt-test");
    let permitted = permitted_accounts_for_model(&pool, &principal.group_id, "public-model")
        .await
        .unwrap();
    assert_eq!(permitted, vec!["account-custom", "account-default"]);
    assert!(
        permitted_accounts_for_model(&pool, &principal.group_id, "gpt-test")
            .await
            .unwrap()
            .is_empty()
    );
    let reservation = crate::saas::billing::reserve(&pool, &principal, "public-model", 2, 2)
        .await
        .unwrap();
    assert_eq!(reservation.upstream_model, "gpt-test");
    assert_eq!(reservation.credential_ids, permitted);
    let settlement = crate::saas::billing::settle(
        &pool,
        &reservation.request_id,
        Some(crate::saas::billing::BillableUsage {
            input_tokens: 2,
            output_tokens: 2,
            ..Default::default()
        }),
        true,
    )
    .await
    .unwrap();
    assert_eq!(settlement.price_usd_micros, Some(6));
}

#[tokio::test]
async fn keys_are_hashed_owner_scoped_and_irrevocably_group_bound() {
    let pool = repository::test_pool().await;
    let (user_id, group_id) = repository::test_user_group(&pool).await;
    let key = user(
        &pool,
        &user_id,
        "keys.create",
        json!({"name":"first", "groupId":group_id}),
    )
    .await
    .unwrap();
    let plain = key["plaintextKey"].as_str().unwrap();
    assert!(plain.starts_with("sk-saas-"));
    let listed = user(&pool, &user_id, "keys.list", json!({})).await.unwrap();
    assert!(!listed.to_string().contains(plain));
    assert!(user(
        &pool,
        "other-user",
        "keys.update",
        json!({"id":key["id"],"name":"stolen"})
    )
    .await
    .is_err());
    assert!(user(
        &pool,
        &user_id,
        "keys.update",
        json!({"id":key["id"],"groupId":"other"})
    )
    .await
    .is_err());
    let rotated = user(&pool, &user_id, "keys.rotate", json!({"id":key["id"]}))
        .await
        .unwrap();
    assert!(crate::saas::billing::authenticate_key(&pool, plain)
        .await
        .is_err());
    assert!(crate::saas::billing::authenticate_key(
        &pool,
        rotated["plaintextKey"].as_str().unwrap()
    )
    .await
    .is_ok());
    user(
        &pool,
        &user_id,
        "keys.update",
        json!({"id":key["id"],"status":"revoked"}),
    )
    .await
    .unwrap();
    assert!(user(
        &pool,
        &user_id,
        "keys.update",
        json!({"id":key["id"],"status":"active"})
    )
    .await
    .is_err());
}

#[tokio::test]
async fn subscription_redeem_codes_activate_a_plan_without_crediting_balance() {
    let pool = repository::test_pool().await;
    let (user_id, _) = repository::test_user_group(&pool).await;
    let plan = admin(
        &pool,
        "subscriptions.plans.save",
        json!({
            "name":"Trial card",
            "kind":"trial",
            "durationDays":1,
            "quotaMicros":1_000_000,
            "priceMicros":0
        }),
    )
    .await
    .unwrap();
    let codes = admin(
        &pool,
        "codes.create",
        json!({"count":1,"amountMicros":0,"subscriptionPlanId":plan["id"]}),
    )
    .await
    .unwrap();
    let result = user(
        &pool,
        &user_id,
        "redeem",
        json!({"code":codes["items"][0]["plaintextCode"]}),
    )
    .await
    .unwrap();
    assert_eq!(result["amountMicros"], 0);
    assert_eq!(result["subscription"]["planId"], plan["id"]);
    let subscriptions = user(&pool, &user_id, "subscriptions.list", json!({}))
        .await
        .unwrap();
    assert_eq!(subscriptions["total"], 1);
    let wallet_entries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM saas_wallet_ledger WHERE user_id=? AND kind='redeem'",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(wallet_entries, 0);
}

#[tokio::test]
async fn external_account_key_reports_balance_and_daily_subscriptions() {
    let pool = repository::test_pool().await;
    let (user_id, _) = repository::test_user_group(&pool).await;
    let plan = admin(
        &pool,
        "subscriptions.plans.save",
        json!({
            "name":"Month card",
            "kind":"month",
            "durationDays":30,
            "quotaMicros":1_000_000,
            "priceMicros":1_000_000
        }),
    )
    .await
    .unwrap();
    admin(
        &pool,
        "subscriptions.grant",
        json!({"userId":user_id,"planId":plan["id"]}),
    )
    .await
    .unwrap();
    admin(
        &pool,
        "users.credit",
        json!({"userId":user_id,"amountMicros":1_000_000,"reason":"External query test"}),
    )
    .await
    .unwrap();
    let key = user(&pool, &user_id, "external-key.rotate", json!({}))
        .await
        .unwrap();
    let authenticated =
        crate::saas::domain::external::authenticate(&pool, key["plaintextKey"].as_str().unwrap())
            .await
            .unwrap();
    assert_eq!(authenticated, user_id);
    let usage = crate::saas::domain::external::usage(&pool, &user_id)
        .await
        .unwrap();
    assert_eq!(usage["isValid"], true);
    assert_eq!(usage["remaining"], 2.0);
    assert_eq!(usage["balance"], 1.0);
    assert_eq!(usage["subscription"]["dailyRemainingMicros"], 1_000_000);
    let subscriptions = crate::saas::domain::external::subscriptions(&pool, &user_id)
        .await
        .unwrap();
    assert_eq!(subscriptions["items"].as_array().unwrap().len(), 1);
    assert_eq!(subscriptions["items"][0]["todayRemainingMicros"], 1_000_000);
}
