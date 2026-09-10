use super::*;
use crate::saas::logs::canonical_test_directory;
use crate::saas::{domain, repository};
use crate::services::route_proxy_service::{build_proxy_state, RouteProxyRuntimeState};
use axum::{routing::post, Router};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

async fn fixture(
    streaming: bool,
) -> (
    SqlitePool,
    SaasRuntime,
    ProxyAppState,
    HeaderMap,
    tempfile::TempDir,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<()>,
) {
    let pool = repository::test_pool().await;
    let (user_id, group_id) = repository::test_user_group(&pool).await;
    sqlx::query("UPDATE saas_users SET balance_micros=1000000 WHERE id=?")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let directory = canonical_test_directory();
    sqlx::query("UPDATE saas_settings SET value_json=json_set(value_json,'$.logs',json(?)) WHERE key='config'")
        .bind(json!({"directory":directory.path()}).to_string()).execute(&pool).await.unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let upstream_hits = hits.clone();
    let image_hits = hits.clone();
    let app = Router::new()
        .route("/v1/images/generations", post(move |Json(payload):Json<Value>| {
            let image_hits = image_hits.clone();
            async move {
                image_hits.fetch_add(1, Ordering::SeqCst);
                assert_eq!(payload["model"], "gpt-test");
                Json(json!({"data":[{"b64_json":"aGVsbG8="},{"b64_json":"d29ybGQ="}]})).into_response()
            }
        }))
        .route("/v1/chat/completions", post(move |Json(payload):Json<Value>| {
        let upstream_hits = upstream_hits.clone();
        async move {
            upstream_hits.fetch_add(1,Ordering::SeqCst);
            assert_eq!(payload["model"],"gpt-test");
            if streaming {
                ([(header::CONTENT_TYPE,"text/event-stream")],
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":30,\"prompt_tokens_details\":{\"cached_tokens\":50}}}\n\ndata: [DONE]\n\n").into_response()
            } else {
                Json(json!({"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":120,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":50}}})).into_response()
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    sqlx::query("UPDATE route_credentials SET secret_payload_json=?,config_json=? WHERE id IN ('account-default','account-custom')")
        .bind(r#"{"api_key":"upstream-test-key"}"#)
        .bind(json!({"base_url":format!("http://{address}/v1"),"interface_format":"openai","model_mappings":[{"from":"gpt-test","to":"gpt-test","capabilities":["image.generate"]}]}).to_string())
        .execute(&pool).await.unwrap();
    let mut extension = repository::test_group_payload();
    extension["models"][0]["model"] = json!("public-model");
    extension["models"][0]["upstreamModel"] = json!("gpt-test");
    extension["models"][0]["imagePriceMicros"] = json!(25_000);
    domain::admin(&pool, "groups.save", extension)
        .await
        .unwrap();
    let key = domain::user(
        &pool,
        &user_id,
        "keys.create",
        json!({"name":"request","groupId":group_id}),
    )
    .await
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {}", key["plaintextKey"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    headers.insert("x-ai-switch-platform", "claude".parse().unwrap());
    let runtime = SaasRuntime::default();
    let proxy = build_proxy_state(pool.clone(), &RouteProxyRuntimeState::default());
    (pool, runtime, proxy, headers, directory, hits, task)
}

#[tokio::test]
async fn ordinary_and_sse_saas_requests_bill_once_and_never_write_sqlite_details() {
    for streaming in [false, true] {
        let (pool, runtime, proxy, headers, _directory, hits, task) = fixture(streaming).await;
        let response = execute(&pool,&runtime,proxy,Method::POST,headers,"/v1/chat/completions".parse().unwrap(),
            Body::from(json!({"model":"public-model","messages":[{"role":"user","content":"hi"}],"max_tokens":50,"stream":streaming}).to_string())).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert!(!body.is_empty());
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let ledger: (i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(amount_micros),0) FROM saas_wallet_ledger WHERE kind='usage'").fetch_one(&pool).await.unwrap();
        assert_eq!(ledger, (1, -135));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usage_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        runtime.logs.shutdown().await.unwrap();
        let logs = runtime
            .logs
            .query(crate::saas::logs::LogQuery::default())
            .await
            .unwrap();
        assert_eq!(logs.total, 1);
        assert_eq!(logs.items[0].amount_usd_micros, 135);
        assert_eq!(logs.items[0].model, "public-model");
        task.abort();
    }
}

#[tokio::test]
async fn image_requests_use_bound_group_and_bill_successful_images() {
    let (pool, runtime, proxy, headers, _directory, hits, task) = fixture(false).await;
    let response = execute(
        &pool,
        &runtime,
        proxy,
        Method::POST,
        headers,
        "/v1/images/generations".parse().unwrap(),
        Body::from(json!({"model":"public-model","prompt":"a fox","n":2}).to_string()),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["data"].as_array().map(Vec::len), Some(2));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let ledger: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),COALESCE(SUM(amount_micros),0) FROM saas_wallet_ledger WHERE kind='usage'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ledger, (1, -50_000));
    let status: String = sqlx::query_scalar("SELECT status FROM saas_billing_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "settled");
    runtime.logs.shutdown().await.unwrap();
    let logs = runtime
        .logs
        .query(crate::saas::logs::LogQuery::default())
        .await
        .unwrap();
    assert_eq!(logs.items[0].amount_usd_micros, 50_000);
    assert_eq!(logs.items[0].model, "public-model");
    task.abort();
}

#[tokio::test]
async fn dropped_response_is_pending_review_instead_of_silently_refunded() {
    let (pool, runtime, proxy, headers, _directory, _hits, task) = fixture(true).await;
    let response = execute(&pool,&runtime,proxy,Method::POST,headers,"/v1/chat/completions".parse().unwrap(),
        Body::from(json!({"model":"public-model","messages":[{"role":"user","content":"hi"}],"max_tokens":50,"stream":true}).to_string())).await.unwrap();
    drop(response);
    for _ in 0..100 {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM saas_billing_reservations WHERE status='pending_review'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if pending == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let status: String = sqlx::query_scalar("SELECT status FROM saas_billing_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending_review");
    runtime.logs.shutdown().await.unwrap();
    task.abort();
}
