use super::*;
use crate::database::{create_memory_pool, run_migrations};

fn completed_response(body: &[u8]) -> Option<Value> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if value.get("status").and_then(Value::as_str) == Some("completed") {
            return Some(value);
        }
    }
    std::str::from_utf8(body).ok()?.lines().find_map(|line| {
        let data = line.trim().strip_prefix("data:")?.trim();
        let value = serde_json::from_str::<Value>(data).ok()?;
        (value.get("type").and_then(Value::as_str) == Some("response.completed"))
            .then(|| value.get("response").cloned())
            .flatten()
    })
}

#[tokio::test]
#[ignore = "requires explicit live Responses DB, credential and model env vars; uses upstream quota"]
async fn live_responses_encrypted_content_recovery() {
    let database = std::env::var("AI_SWITCH_LIVE_RESPONSES_DB").expect("set live database path");
    let source_id =
        std::env::var("AI_SWITCH_LIVE_RESPONSES_CREDENTIAL").expect("set live credential ID");
    let model = std::env::var("AI_SWITCH_LIVE_RESPONSES_MODEL").expect("set live model");
    let source = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(database)
                .read_only(true)
                .create_if_missing(false),
        )
        .await
        .expect("open read-only source");
    let (platform, config_json, secret_json) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT platform, config_json, secret_payload_json FROM route_credentials WHERE id = ?",
    )
    .bind(source_id)
    .fetch_one(&source)
    .await
    .expect("read live credential");
    source.close().await;
    assert_eq!(platform, "codex");
    let secret: Value = serde_json::from_str(&secret_json).expect("credential secret");
    let api_key = secret["api_key"]
        .as_str()
        .filter(|key| !key.is_empty())
        .expect("API key");
    let mut config: Value = serde_json::from_str(&config_json).expect("credential config");
    assert_eq!(config["interface_format"], "openai-responses");
    config["failure_policy"] = json!({"retry_count": 0});
    let pool = create_memory_pool().await.expect("isolated pool");
    run_migrations(&pool).await.expect("isolated migrations");
    let credential = RouteCredentialRepository::create(
        &pool,
        "codex",
        "api",
        "live-probe",
        None,
        "ok",
        None,
        &secret_json,
        &config.to_string(),
        "{}",
    )
    .await
    .expect("isolated credential");
    RoutePoolRepository::replace_members(&pool, "codex", std::slice::from_ref(&credential.id))
        .await
        .expect("isolated membership");
    let route_key = RouteProxyKeyRepository::ensure_platform_key(
        &pool,
        "codex",
        "sk-ai-switch-live-isolated-probe",
    )
    .await
    .expect("isolated route key");
    let runtime = RouteProxyRuntimeState::default();
    let mut state = build_proxy_state(pool.clone(), &runtime);
    state.upstream_timeouts = OutboundTimeouts {
        total: Some(Duration::from_secs(50)),
        connect: Some(Duration::from_secs(15)),
        read: Some(Duration::from_secs(40)),
    };
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("isolated port");
    let address = listener.local_addr().expect("isolated address");
    let app = Router::new().fallback(any(proxy_handler)).with_state(state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("local client");
    let session_id = uuid::Uuid::new_v4().to_string();
    let turn_id = uuid::Uuid::new_v4().to_string();
    let installation_id = uuid::Uuid::new_v4().to_string();
    let window_id = format!("{session_id}:0");
    let turn_metadata = json!({
        "session_id": session_id, "thread_id": session_id, "turn_id": turn_id,
        "installation_id": installation_id, "window_id": window_id,
        "request_kind": "turn", "thread_source": "user",
        "turn_started_at_unix_ms": Utc::now().timestamp_millis()
    })
    .to_string();
    let mut request = json!({
        "model": model,
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Connectivity test. Do not use tools. Reply with only the answer."}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "What is 19 times 23?"}]}
        ],
        "tool_choice": "auto", "parallel_tool_calls": false,
        "reasoning": {"effort": "low", "context": "all_turns"},
        "store": false, "stream": true, "max_output_tokens": 1024,
        "text": {"verbosity": "low"}, "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": session_id,
        "client_metadata": {
            "session_id": session_id, "thread_id": session_id, "turn_id": turn_id,
            "x-codex-installation-id": installation_id,
            "x-codex-window-id": window_id, "x-codex-turn-metadata": turn_metadata
        }
    });
    let mut completed = Vec::new();
    for phase in ["baseline", "encrypted-replay"] {
        let response = client
            .post(format!("http://{address}/v1/responses"))
            .bearer_auth(&route_key)
            .header("user-agent", client_identity::codex_cli_user_agent())
            .header("originator", client_identity::CODEX_CLI_ORIGINATOR)
            .header("accept", "text/event-stream")
            .header("x-openai-internal-codex-responses-lite", "true")
            .header("x-codex-beta-features", "remote_compaction_v2")
            .header("x-codex-window-id", &window_id)
            .header("x-codex-turn-metadata", &turn_metadata)
            .header("x-client-request-id", &session_id)
            .header("session-id", &session_id)
            .header("thread-id", &session_id)
            .json(&request)
            .send()
            .await
            .expect("isolated live response");
        let status = response.status();
        let bytes = response.bytes().await.expect("live response body");
        let result = completed_response(&bytes);
        eprintln!(
            "live phase={phase} http={status} completed={} session={session_id}",
            result.is_some()
        );
        let Some(result) = result else { break };
        let has_text = result["output"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["content"].as_array().is_some_and(|content| {
                    content.iter().any(|part| {
                        part["type"] == "output_text"
                            && part["text"].as_str().is_some_and(|text| !text.is_empty())
                    })
                })
            })
        });
        if !has_text {
            eprintln!("live phase={phase} completed response has no text output");
            break;
        }
        completed.push(result);
        if phase == "baseline" {
            request["input"].as_array_mut().unwrap().insert(0, json!({
                "type": "reasoning", "id": "rs_replayed_probe", "summary": [],
                "encrypted_content": "gAAAAABforeign-encrypted-content-for-isolated-recovery-test"
            }));
        }
    }
    server.abort();
    let _ = server.await;
    let records: Vec<String> =
        sqlx::query_scalar("SELECT metadata_json FROM usage_events ORDER BY created_at")
            .fetch_all(&pool)
            .await
            .expect("isolated events");
    let mut statuses = Vec::new();
    let mut errors = Vec::new();
    for record in records {
        let metadata: Value = serde_json::from_str(&record).expect("event metadata");
        let response: Value = metadata["response_body"]
            .as_str()
            .and_then(|body| serde_json::from_str(body).ok())
            .unwrap_or(Value::Null);
        let code = response
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("");
        let message = response
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("");
        let request_id = message
            .split("request id: ")
            .nth(1)
            .and_then(|tail| tail.split(')').next())
            .unwrap_or("");
        let transport_error = if metadata["status"].is_null() {
            metadata["error_message"]
                .as_str()
                .unwrap_or("")
                .replace(api_key, "[REDACTED]")
        } else {
            String::new()
        };
        eprintln!("live upstream status={} code={code} request_id={request_id} transport={transport_error}", metadata["status"]);
        statuses.push(metadata["status"].as_u64().unwrap_or(0));
        errors.push(code.to_string());
    }
    assert_eq!(
        completed.len(),
        2,
        "live baseline and recovered request must both complete"
    );
    assert_eq!(
        statuses,
        vec![200, 400, 200],
        "must observe one rewrite retry"
    );
    assert_eq!(errors[1], "invalid_encrypted_content");
    let account = RouteCredentialRepository::get(&pool, &credential.id)
        .await
        .expect("isolated account");
    assert_eq!(account.status, "ok");
    assert_eq!(account.transient_failure_count, 0);
}
