# Route Model Connectivity Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the existing "测试路由" button perform a real upstream model connectivity test and show the fixed test input plus model response details.

**Architecture:** Add a focused Rust service for manual model connectivity tests, reusing route credential selection and upstream request construction patterns without changing normal proxy logging. Expose the service through a new Tauri/web command and wire the existing compact "测试路由" UI button to the new command, showing a result card and preserving the existing request statistics detail flow.

**Tech Stack:** Rust 2021, Tauri 2 commands, Axum test server utilities already in dependencies, Reqwest with rustls, SQLite via sqlx, React 18, TypeScript, TanStack Query, Vitest, Testing Library.

## Execution Status

- Implemented on 2026-07-19.
- Verified with focused backend/frontend checks, full `cargo test`, `cargo check`, `pnpm typecheck`, and full `pnpm test:run`.
- The final UI keeps the button text `测试路由` and only requires at least one pool member before the test can run.

## Global Constraints

- Work directly on `main` by default. Do not create or switch to feature branches/worktrees unless the user explicitly asks for a separate branch, worktree, or isolation.
- Do not log normal user proxy request bodies or response bodies.
- Do not store API keys, access tokens, refresh tokens, `Authorization`, or `x-api-key` values.
- Do not remove `route_pool_route_once`; keep it for existing internal tests and compatibility.
- Do not implement a full prompt playground or arbitrary chat UI.
- Do not require users to type a prompt in the first version.
- Keep the button label as `测试路由`; use result-card copy to clarify model connectivity.
- `测试路由` must call the app's internal backend command directly; it must not require the local route proxy to be running or route config files to be written first.
- Do not touch unrelated dirty Vibe/skin files currently present in the working tree.

---

## File Structure

- Create `src-tauri/src/services/route_model_test_service.rs`: owns fixed prompt construction, response parsing, real HTTP send, usage-event metadata, and service tests.
- Modify `src-tauri/src/services/mod.rs`: exports the new service module.
- Modify `src-tauri/src/models/route_pool.rs`: adds `RoutePoolModelTestRequest` and `RoutePoolModelTestOutcome`.
- Modify `src-tauri/src/commands/route_pool_commands.rs`: adds Tauri command `route_pool_test_model`.
- Modify `src-tauri/src/lib.rs`: registers `route_pool_test_model`.
- Modify `src-tauri/src/web/handlers/mod.rs`: adds web command dispatch for `route_pool_test_model`.
- Modify `src/lib/api/types.ts`: adds frontend model-test request/outcome types.
- Modify `src/lib/api/client.ts`: adds `routePoolTestModel`.
- Modify `src/screens/AccountsScreen.tsx`: points "测试路由" at the real model test and renders result details.
- Modify `tests/AccountsScreen.test.tsx`: covers success/failure UI and metadata details.

### Task 1: Backend Model-Test Pure Functions

**Files:**
- Create: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Consumes: `SelectedCredential`, `build_upstream_request`, and `extract_token_count` / `extract_cost_micros` from `crate::services::route_proxy_service`.
- Produces: `RouteModelTestService`, `build_model_test_request`, `extract_model_test_response_text`, `truncate_response_body`, and constants `MODEL_TEST_PROMPT`, `MODEL_TEST_RESPONSE_LIMIT`.

- [ ] **Step 1: Create failing Rust tests for request construction and response extraction**

Create `src-tauri/src/services/route_model_test_service.rs` with this initial test-focused content:

```rust
use crate::models::route_credential::ModelMapping;
use crate::services::route_proxy_service::SelectedCredential;
use serde_json::{json, Value};

pub struct RouteModelTestService;

pub const MODEL_TEST_PROMPT: &str = "Reply with exactly: ai-switch-ok";
pub const MODEL_TEST_RESPONSE_LIMIT: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTestRequestParts {
    pub interface_format: String,
    pub request_path: String,
    pub request_body_json: String,
}

pub fn build_model_test_request(
    credential: &SelectedCredential,
    platform: &str,
) -> Result<ModelTestRequestParts, String> {
    let _ = (credential, platform);
    Err("not implemented".to_string())
}

pub fn extract_model_test_response_text(interface_format: &str, body: &str) -> Option<String> {
    let _ = (interface_format, body);
    None
}

pub fn truncate_response_body(body: &[u8]) -> String {
    String::from_utf8_lossy(&body[..body.len().min(MODEL_TEST_RESPONSE_LIMIT)]).to_string()
}

fn api_credential(interface_format: &str) -> SelectedCredential {
    SelectedCredential {
        id: "cred-api".to_string(),
        platform: "codex".to_string(),
        kind: "api".to_string(),
        display_name: "API Account".to_string(),
        secret_payload_json: r#"{"api_key":"sk-test"}"#.to_string(),
        config_json: json!({
            "base_url": "https://api.example.com/v1",
            "interface_format": interface_format,
            "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
        })
        .to_string(),
    }
}

fn official_credential(platform: &str) -> SelectedCredential {
    SelectedCredential {
        id: "cred-official".to_string(),
        platform: platform.to_string(),
        kind: "official".to_string(),
        display_name: "Official Account".to_string(),
        secret_payload_json: r#"{"access_token":"at"}"#.to_string(),
        config_json: "{}".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_openai_chat_test_request() {
        let request = build_model_test_request(&api_credential("openai"), "codex")
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai");
        assert_eq!(request.request_path, "/chat/completions");
        assert_eq!(body.pointer("/model").and_then(Value::as_str), Some("gpt-5"));
        assert_eq!(
            body.pointer("/messages/0/content").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
        assert_eq!(body.pointer("/max_tokens").and_then(Value::as_i64), Some(16));
    }

    #[test]
    fn builds_openai_responses_test_request() {
        let request = build_model_test_request(&api_credential("openai-responses"), "codex")
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "openai-responses");
        assert_eq!(request.request_path, "/responses");
        assert_eq!(body.pointer("/model").and_then(Value::as_str), Some("gpt-5"));
        assert_eq!(body.pointer("/input").and_then(Value::as_str), Some(MODEL_TEST_PROMPT));
        assert_eq!(body.pointer("/max_output_tokens").and_then(Value::as_i64), Some(16));
    }

    #[test]
    fn builds_anthropic_test_request_for_official_claude() {
        let request = build_model_test_request(&official_credential("claude"), "claude")
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "anthropic");
        assert_eq!(request.request_path, "/v1/messages");
        assert_eq!(
            body.pointer("/model").and_then(Value::as_str),
            Some("claude-sonnet-4-20250514"),
        );
        assert_eq!(
            body.pointer("/messages/0/content").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
    }

    #[test]
    fn builds_gemini_test_request_and_uses_mapping_target_in_path() {
        let request = build_model_test_request(&api_credential("gemini"), "gemini")
            .expect("request");
        let body: Value = serde_json::from_str(&request.request_body_json).expect("json");

        assert_eq!(request.interface_format, "gemini");
        assert_eq!(request.request_path, "/v1beta/models/up-gpt:generateContent");
        assert_eq!(
            body.pointer("/contents/0/parts/0/text").and_then(Value::as_str),
            Some(MODEL_TEST_PROMPT),
        );
        assert_eq!(
            body.pointer("/generationConfig/maxOutputTokens").and_then(Value::as_i64),
            Some(16),
        );
    }

    #[test]
    fn extracts_model_text_from_supported_response_shapes() {
        assert_eq!(
            extract_model_test_response_text(
                "openai",
                r#"{"choices":[{"message":{"content":"ai-switch-ok"}}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text("openai-responses", r#"{"output_text":"ai-switch-ok"}"#)
                .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text(
                "anthropic",
                r#"{"content":[{"type":"text","text":"ai-switch-ok"}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
        assert_eq!(
            extract_model_test_response_text(
                "gemini",
                r#"{"candidates":[{"content":{"parts":[{"text":"ai-switch-ok"}]}}]}"#,
            )
            .as_deref(),
            Some("ai-switch-ok"),
        );
    }

    #[test]
    fn truncates_response_body_to_safe_limit() {
        let body = vec![b'a'; MODEL_TEST_RESPONSE_LIMIT + 10];
        assert_eq!(truncate_response_body(&body).len(), MODEL_TEST_RESPONSE_LIMIT);
    }
}
```

- [ ] **Step 2: Export the new module**

In `src-tauri/src/services/mod.rs`, add:

```rust
pub mod route_model_test_service;
```

- [ ] **Step 3: Run the focused Rust tests and verify they fail**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service
```

Expected: FAIL. The request construction tests fail with `not implemented`, and response extraction tests return `None`.

- [ ] **Step 4: Implement request construction and response extraction**

Replace the initial helper implementations in `src-tauri/src/services/route_model_test_service.rs` with:

```rust
fn parse_json_object(raw: &str, label: &str) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("Route credential {label} JSON is invalid: {err}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!("Route credential {label} JSON must be an object"))
    }
}

fn string_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

fn model_mappings(config: &Value) -> Vec<ModelMapping> {
    config
        .get("model_mappings")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ModelMapping>>(value).ok())
        .unwrap_or_default()
}

fn interface_format_for(credential: &SelectedCredential, platform: &str, config: &Value) -> String {
    if credential.kind == "api" {
        return string_value(config, "interface_format")
            .unwrap_or("openai")
            .to_string();
    }

    match platform {
        "claude" => "anthropic".to_string(),
        "gemini" => "gemini".to_string(),
        _ => "openai".to_string(),
    }
}

fn default_model_for(interface_format: &str) -> &'static str {
    match interface_format {
        "anthropic" | "anthropic-messages" => "claude-sonnet-4-20250514",
        "gemini" => "gemini-2.5-flash",
        _ => "gpt-5",
    }
}

fn request_model(interface_format: &str, mappings: &[ModelMapping]) -> String {
    mappings
        .first()
        .map(|mapping| mapping.from.trim())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| default_model_for(interface_format))
        .to_string()
}

fn gemini_path_model(mappings: &[ModelMapping]) -> String {
    mappings
        .first()
        .map(|mapping| mapping.to.trim())
        .filter(|model| !model.is_empty())
        .unwrap_or("gemini-2.5-flash")
        .to_string()
}

pub fn build_model_test_request(
    credential: &SelectedCredential,
    platform: &str,
) -> Result<ModelTestRequestParts, String> {
    let config = parse_json_object(&credential.config_json, "config")?;
    let interface_format = interface_format_for(credential, platform, &config);
    let mappings = model_mappings(&config);
    let model = request_model(&interface_format, &mappings);

    let (request_path, request_body) = match interface_format.as_str() {
        "openai" => (
            "/chat/completions".to_string(),
            json!({
                "model": model,
                "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
                "temperature": 0,
                "max_tokens": 16
            }),
        ),
        "openai-responses" => (
            "/responses".to_string(),
            json!({
                "model": model,
                "input": MODEL_TEST_PROMPT,
                "temperature": 0,
                "max_output_tokens": 16
            }),
        ),
        "anthropic" | "anthropic-messages" => (
            "/v1/messages".to_string(),
            json!({
                "model": model,
                "messages": [{"role": "user", "content": MODEL_TEST_PROMPT}],
                "max_tokens": 16
            }),
        ),
        "gemini" => (
            format!("/v1beta/models/{}:generateContent", gemini_path_model(&mappings)),
            json!({
                "contents": [{
                    "role": "user",
                    "parts": [{"text": MODEL_TEST_PROMPT}]
                }],
                "generationConfig": {
                    "temperature": 0,
                    "maxOutputTokens": 16
                }
            }),
        ),
        other => return Err(format!("Unsupported interface format: {other}")),
    };

    Ok(ModelTestRequestParts {
        interface_format,
        request_path,
        request_body_json: serde_json::to_string_pretty(&request_body)
            .map_err(|err| format!("Could not serialize test request body: {err}"))?,
    })
}

fn text_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str).map(str::trim).filter(|item| !item.is_empty())
}

pub fn extract_model_test_response_text(interface_format: &str, body: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(body).ok()?;

    if matches!(interface_format, "openai" | "openai-responses") {
        if let Some(text) = text_at(&value, "/choices/0/message/content") {
            return Some(text.to_string());
        }
        if let Some(text) = text_at(&value, "/output_text") {
            return Some(text.to_string());
        }
        if let Some(items) = value.pointer("/output").and_then(Value::as_array) {
            for item in items {
                if let Some(content_items) = item.get("content").and_then(Value::as_array) {
                    for content in content_items {
                        if let Some(text) = content.get("text").and_then(Value::as_str) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if matches!(interface_format, "anthropic" | "anthropic-messages") {
        if let Some(text) = text_at(&value, "/content/0/text") {
            return Some(text.to_string());
        }
    }

    if interface_format == "gemini" {
        if let Some(text) = text_at(&value, "/candidates/0/content/parts/0/text") {
            return Some(text.to_string());
        }
    }

    None
}
```

- [ ] **Step 5: Run focused Rust tests and verify they pass**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service
```

Expected: PASS for all `route_model_test_service` pure-function tests.

### Task 2: Backend Service, Commands, And Usage Recording

**Files:**
- Modify: `src-tauri/src/models/route_pool.rs`
- Modify: `src-tauri/src/services/route_model_test_service.rs`
- Modify: `src-tauri/src/commands/route_pool_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`

**Interfaces:**
- Consumes: `RoutePoolRepository::next_cursor_index`, `RoutePoolRepository::save_cursor_index`, `RoutePoolRepository::insert_usage_event`, `RoutePoolRepository::stats`, `normalize_platform`, `build_upstream_request`.
- Produces: `RoutePoolModelTestRequest`, `RoutePoolModelTestOutcome`, `RouteModelTestService::test_model(pool, request)`, Tauri/web command `route_pool_test_model`.

- [ ] **Step 1: Add backend request/outcome model types**

In `src-tauri/src/models/route_pool.rs`, after `RoutePoolRouteOutcome`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutePoolModelTestRequest {
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePoolModelTestOutcome {
    pub platform: String,
    pub selected_account_id: String,
    pub selected_account_name: String,
    pub interface_format: String,
    pub request_path: String,
    pub request_body_json: String,
    pub response_status: Option<u16>,
    pub response_body: String,
    pub response_text: Option<String>,
    pub error_message: Option<String>,
    pub success: bool,
    pub duration_ms: i64,
    pub stats: RoutePoolStats,
}
```

- [ ] **Step 2: Add service tests for real HTTP outcomes and metadata**

Append these tests to the `#[cfg(test)] mod tests` block in `src-tauri/src/services/route_model_test_service.rs`:

```rust
use crate::database::{create_memory_pool, run_migrations};
use crate::database::repositories::route_credential_repository::RouteCredentialRepository;
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::models::route_pool::{RoutePoolModelTestRequest, SetRoutePoolMembersInput};
use crate::services::route_pool_service::RoutePoolService;
use axum::{routing::post, Json, Router};
use serde_json::json;
use tokio::net::TcpListener;

async fn start_json_test_server(status: axum::http::StatusCode, body: Value) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || async move { (status, Json(body.clone())) }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    format!("http://{addr}/v1")
}

async fn create_api_credential(pool: &sqlx::SqlitePool, base_url: &str) -> String {
    RouteCredentialRepository::create(
        pool,
        "codex",
        "api",
        "API Account",
        None,
        "ok",
        None,
        r#"{"api_key":"sk-test"}"#,
        &json!({
            "base_url": base_url,
            "interface_format": "openai",
            "model_mappings": [{"from":"gpt-5","to":"up-gpt"}]
        })
        .to_string(),
        r#"{"config_toml":""}"#,
    )
    .await
    .expect("credential")
    .id
}

#[tokio::test]
async fn test_model_records_success_metadata_and_usage() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");
    let base_url = start_json_test_server(
        axum::http::StatusCode::OK,
        json!({
            "choices": [{"message": {"content": "ai-switch-ok"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "cost_micros": 42}
        }),
    )
    .await;
    let credential_id = create_api_credential(&pool, &base_url).await;
    RoutePoolService::set_members(
        &pool,
        SetRoutePoolMembersInput {
            platform: "codex".to_string(),
            account_ids: vec![credential_id.clone()],
        },
    )
    .await
    .expect("members");

    let outcome = RouteModelTestService::test_model(
        &pool,
        RoutePoolModelTestRequest {
            platform: "codex".to_string(),
        },
    )
    .await
    .expect("outcome");

    assert!(outcome.success);
    assert_eq!(outcome.selected_account_id, credential_id);
    assert_eq!(outcome.interface_format, "openai");
    assert_eq!(outcome.request_path, "/chat/completions");
    assert_eq!(outcome.response_status, Some(200));
    assert_eq!(outcome.response_text.as_deref(), Some("ai-switch-ok"));
    assert!(outcome.request_body_json.contains(MODEL_TEST_PROMPT));
    assert!(outcome.response_body.contains("ai-switch-ok"));
    assert_eq!(outcome.stats.request_count, 1);
    assert_eq!(outcome.stats.token_count, 8);
    assert_eq!(outcome.stats.cost_micros, 42);

    let stats = RoutePoolRepository::stats(&pool, "codex", None, 1, 20)
        .await
        .expect("stats");
    assert_eq!(stats.requests.len(), 1);
    assert_eq!(stats.requests[0].source_label, "route_pool_model_test");
    assert!(stats.requests[0].metadata_json.contains("model_connectivity"));
    assert!(stats.requests[0].metadata_json.contains("request_body_json"));
    assert!(stats.requests[0].metadata_json.contains("response_body"));
    assert!(!stats.requests[0].metadata_json.contains("sk-test"));
}

#[tokio::test]
async fn test_model_returns_failure_outcome_for_http_error() {
    let pool = create_memory_pool().await.expect("pool");
    run_migrations(&pool).await.expect("migrations");
    let base_url = start_json_test_server(
        axum::http::StatusCode::UNAUTHORIZED,
        json!({"error": {"message": "bad key"}}),
    )
    .await;
    let credential_id = create_api_credential(&pool, &base_url).await;
    RoutePoolService::set_members(
        &pool,
        SetRoutePoolMembersInput {
            platform: "codex".to_string(),
            account_ids: vec![credential_id],
        },
    )
    .await
    .expect("members");

    let outcome = RouteModelTestService::test_model(
        &pool,
        RoutePoolModelTestRequest {
            platform: "codex".to_string(),
        },
    )
    .await
    .expect("outcome");

    assert!(!outcome.success);
    assert_eq!(outcome.response_status, Some(401));
    assert!(outcome.response_body.contains("bad key"));
    assert!(outcome.error_message.is_none());
    assert_eq!(outcome.stats.request_count, 1);
}
```

- [ ] **Step 3: Run the service tests and verify they fail before implementation**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service
```

Expected: FAIL because `RouteModelTestService::test_model` and `RoutePoolModelTestRequest` imports are not implemented yet.

- [ ] **Step 4: Implement selected credential loading and service execution**

In `src-tauri/src/services/route_model_test_service.rs`, add these imports at the top:

```rust
use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::route_pool::{RoutePoolModelTestOutcome, RoutePoolModelTestRequest};
use crate::services::route_pool_service::normalize_platform;
use crate::services::route_proxy_service::{
    build_upstream_request, extract_cost_micros, extract_token_count, pick_credential,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use sqlx::{Row, SqlitePool};
use std::time::Instant;
use tokio::time::Duration;
```

Add these functions before the test module:

```rust
async fn load_pool_credentials(
    pool: &SqlitePool,
    platform: &str,
) -> Result<Vec<SelectedCredential>, AppError> {
    let rows = sqlx::query(
        "SELECT c.id, c.platform, c.kind, c.display_name, c.secret_payload_json, c.config_json
         FROM route_pool_members rpm
         INNER JOIN route_credentials c ON c.id = rpm.route_credential_id
         WHERE rpm.platform = ? AND rpm.enabled = 1 AND c.status = 'ok'
         ORDER BY rpm.sort_order ASC, rpm.created_at ASC",
    )
    .bind(platform)
    .fetch_all(pool)
    .await
    .map_err(|err| AppError::Database {
        code: "database.route_model_test_credentials",
        message: "Could not load route credentials for model test".to_string(),
        details: Some(err.to_string()),
        recoverable: true,
    })?;

    Ok(rows
        .into_iter()
        .map(|row| SelectedCredential {
            id: row.get("id"),
            platform: row.get("platform"),
            kind: row.get("kind"),
            display_name: row.get("display_name"),
            secret_payload_json: row.get("secret_payload_json"),
            config_json: row.get("config_json"),
        })
        .collect())
}

fn json_headers() -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("application/json"),
    );
    Ok(headers)
}

fn metadata_json(
    platform: &str,
    credential: &SelectedCredential,
    parts: &ModelTestRequestParts,
    status: Option<u16>,
    success: bool,
    duration_ms: i64,
    response_body: &str,
    response_text: Option<&str>,
    error_message: Option<&str>,
) -> String {
    json!({
        "source": "ui_model_connectivity_test",
        "request_kind": "model_connectivity",
        "platform": platform,
        "route_credential_id": credential.id,
        "route_credential_name": credential.display_name,
        "interface_format": parts.interface_format,
        "path": parts.request_path,
        "status": status,
        "success": success,
        "duration_ms": duration_ms,
        "request_body_json": parts.request_body_json,
        "response_body": response_body,
        "response_text": response_text,
        "error_message": error_message
    })
    .to_string()
}

async fn record_attempt(
    pool: &SqlitePool,
    platform: &str,
    credential: &SelectedCredential,
    parts: &ModelTestRequestParts,
    response_status: Option<u16>,
    response_body: &str,
    response_text: Option<&str>,
    error_message: Option<&str>,
    success: bool,
    duration_ms: i64,
) -> Result<(), AppError> {
    let metadata = metadata_json(
        platform,
        credential,
        parts,
        response_status,
        success,
        duration_ms,
        response_body,
        response_text,
        error_message,
    );
    RoutePoolRepository::insert_usage_event(
        pool,
        &credential.id,
        "route_pool_model_test",
        "request",
        1,
        "count",
        &metadata,
    )
    .await?;
    Ok(())
}

impl RouteModelTestService {
    pub async fn test_model(
        pool: &SqlitePool,
        request: RoutePoolModelTestRequest,
    ) -> Result<RoutePoolModelTestOutcome, AppError> {
        let platform = normalize_platform(&request.platform)?;
        let credentials = load_pool_credentials(pool, &platform).await?;
        if credentials.is_empty() {
            return Err(AppError::Validation {
                code: "validation.route_pool_empty",
                message: "Route pool has no enabled accounts".to_string(),
                details: Some(platform),
                recoverable: true,
            });
        }

        let cursor = RoutePoolRepository::next_cursor_index(pool, &platform).await?;
        let credential = pick_credential(&credentials, cursor)
            .expect("credentials checked as non-empty")
            .clone();
        let next_index = (cursor.rem_euclid(credentials.len() as i64) + 1) % credentials.len() as i64;
        let parts = build_model_test_request(&credential, &platform).map_err(|message| AppError::Validation {
            code: "validation.route_model_test_request",
            message: "Could not build model connectivity test request".to_string(),
            details: Some(message),
            recoverable: true,
        })?;

        let started = Instant::now();
        let mut response_status = None;
        let mut response_body = String::new();
        let mut response_text = None;
        let mut error_message = None;
        let mut success = false;

        let send_result = async {
            let body = parts.request_body_json.as_bytes().to_vec();
            let (target_url, headers, outbound_body) = build_upstream_request(
                &credential,
                &platform,
                &parts.request_path,
                None,
                json_headers()?,
                &body,
            )
            .map_err(|message| AppError::Validation {
                code: "validation.route_model_test_upstream",
                message: "Could not build upstream model test request".to_string(),
                details: Some(message),
                recoverable: true,
            })?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|err| AppError::Validation {
                    code: "network.route_model_test_client",
                    message: "Could not create model test HTTP client".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
            let response = client
                .post(target_url)
                .headers(reqwest::header::HeaderMap::from_iter(headers.into_iter().filter_map(|(name, value)| {
                    name.map(|name| {
                        (
                            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()).expect("header name"),
                            reqwest::header::HeaderValue::from_bytes(value.as_bytes()).expect("header value"),
                        )
                    })
                })))
                .body(outbound_body)
                .send()
                .await
                .map_err(|err| AppError::Validation {
                    code: "network.route_model_test_send",
                    message: "Model connectivity test request failed".to_string(),
                    details: Some(err.to_string()),
                    recoverable: true,
                })?;
            let status = response.status();
            let bytes = response.bytes().await.map_err(|err| AppError::Validation {
                code: "network.route_model_test_response",
                message: "Could not read model connectivity test response".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;
            Ok::<_, AppError>((status.as_u16(), status.is_success(), bytes.to_vec()))
        }
        .await;

        let duration_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;

        match send_result {
            Ok((status, is_success, bytes)) => {
                response_status = Some(status);
                response_body = truncate_response_body(&bytes);
                response_text = extract_model_test_response_text(&parts.interface_format, &response_body);
                success = is_success;

                if let Some(tokens) = extract_token_count(&bytes) {
                    if tokens > 0 {
                        let metadata = metadata_json(
                            &platform,
                            &credential,
                            &parts,
                            response_status,
                            success,
                            duration_ms,
                            &response_body,
                            response_text.as_deref(),
                            None,
                        );
                        RoutePoolRepository::insert_usage_event(
                            pool,
                            &credential.id,
                            "route_pool_model_test",
                            "token",
                            tokens,
                            "token",
                            &metadata,
                        )
                        .await?;
                    }
                }

                if let Some(cost) = extract_cost_micros(&bytes) {
                    if cost > 0 {
                        let metadata = metadata_json(
                            &platform,
                            &credential,
                            &parts,
                            response_status,
                            success,
                            duration_ms,
                            &response_body,
                            response_text.as_deref(),
                            None,
                        );
                        RoutePoolRepository::insert_usage_event(
                            pool,
                            &credential.id,
                            "route_pool_model_test",
                            "cost",
                            cost,
                            "usd_micros",
                            &metadata,
                        )
                        .await?;
                    }
                }
            }
            Err(error) => {
                error_message = Some(error.to_string());
            }
        }

        record_attempt(
            pool,
            &platform,
            &credential,
            &parts,
            response_status,
            &response_body,
            response_text.as_deref(),
            error_message.as_deref(),
            success,
            duration_ms,
        )
        .await?;
        RoutePoolRepository::save_cursor_index(pool, &platform, next_index).await?;

        Ok(RoutePoolModelTestOutcome {
            platform: platform.clone(),
            selected_account_id: credential.id,
            selected_account_name: credential.display_name,
            interface_format: parts.interface_format,
            request_path: parts.request_path,
            request_body_json: parts.request_body_json,
            response_status,
            response_body,
            response_text,
            error_message,
            success,
            duration_ms,
            stats: RoutePoolRepository::stats(pool, &platform, None, 1, 20).await?,
        })
    }
}
```

Use `AppError::Validation` for the `network.route_model_test_*` codes because the current
`AppError` enum has no dedicated network variant.

- [ ] **Step 5: Register Tauri and web commands**

In `src-tauri/src/commands/route_pool_commands.rs`, update imports:

```rust
use crate::models::route_pool::{
    RoutePoolModelTestOutcome, RoutePoolModelTestRequest, RoutePoolRouteOutcome,
    RoutePoolRouteRequest, RoutePoolState, SetRoutePoolMembersInput,
};
use crate::services::route_model_test_service::RouteModelTestService;
```

Then add:

```rust
#[tauri::command]
pub async fn route_pool_test_model(
    state: State<'_, AppState>,
    request: RoutePoolModelTestRequest,
) -> Result<RoutePoolModelTestOutcome, ApiError> {
    RouteModelTestService::test_model(&state.pool, request)
        .await
        .map_err(ApiError::from)
}
```

In `src-tauri/src/lib.rs`, import and register the command:

```rust
use commands::route_pool_commands::{
    get_route_pool, route_pool_route_once, route_pool_test_model, set_route_pool_members,
};
```

Add `route_pool_test_model` next to `route_pool_route_once` in `tauri::generate_handler!`.

In `src-tauri/src/web/handlers/mod.rs`, update imports:

```rust
use crate::models::route_pool::{
    RoutePoolModelTestRequest, RoutePoolRouteRequest, SetRoutePoolMembersInput,
};
use crate::services::route_model_test_service::RouteModelTestService;
```

Then add this match arm after `route_pool_route_once`:

```rust
"route_pool_test_model" => {
    let request: RoutePoolModelTestRequest = parse_arg(&args, "request")?;
    to_value(
        RouteModelTestService::test_model(&state.pool, request)
            .await
            .map_err(to_error)?,
    )
}
```

- [ ] **Step 6: Run backend verification**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service
cargo test --manifest-path src-tauri/Cargo.toml route_pool
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all commands PASS.

### Task 3: Frontend Types, Client, And UI Tests

**Files:**
- Modify: `src/lib/api/types.ts`
- Modify: `src/lib/api/client.ts`
- Modify: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: backend command `route_pool_test_model`.
- Produces: `RoutePoolModelTestRequest`, `RoutePoolModelTestOutcome`, `routePoolTestModel(request)`, mocked success/failure UI tests.

- [ ] **Step 1: Add frontend API types**

In `src/lib/api/types.ts`, after `RoutePoolRouteOutcome`, add:

```ts
export type RoutePoolModelTestRequest = {
  platform: string;
};

export type RoutePoolModelTestOutcome = {
  platform: string;
  selected_account_id: string;
  selected_account_name: string;
  interface_format: string;
  request_path: string;
  request_body_json: string;
  response_status?: number | null;
  response_body: string;
  response_text?: string | null;
  error_message?: string | null;
  success: boolean;
  duration_ms: number;
  stats: RoutePoolStats;
};
```

- [ ] **Step 2: Add frontend API client function**

In `src/lib/api/client.ts`, add `RoutePoolModelTestOutcome` and `RoutePoolModelTestRequest` to the type import block, then add:

```ts
export function routePoolTestModel(request: RoutePoolModelTestRequest): Promise<RoutePoolModelTestOutcome> {
  return invoke("route_pool_test_model", { request });
}
```

- [ ] **Step 3: Update frontend tests to expect real model-test behavior**

In `tests/AccountsScreen.test.tsx`:

Add `routePoolTestModel` to the client imports and `vi.mock("../src/lib/api/client", () => ({ ... }))` object.

Update the type import:

```ts
import type { RouteCredential, RoutePoolModelTestOutcome, RoutePoolStats } from "../src/lib/api/types";
```

Add this fixture near `statsFixture`:

```ts
function modelTestOutcomeFixture(overrides: Partial<RoutePoolModelTestOutcome> = {}): RoutePoolModelTestOutcome {
  return {
    platform: "codex",
    selected_account_id: "cred-official-1",
    selected_account_name: "Team Account",
    interface_format: "openai",
    request_path: "/chat/completions",
    request_body_json: JSON.stringify(
      {
        model: "gpt-5",
        messages: [{ role: "user", content: "Reply with exactly: ai-switch-ok" }],
        temperature: 0,
        max_tokens: 16,
      },
      null,
      2,
    ),
    response_status: 200,
    response_body: "{\"choices\":[{\"message\":{\"content\":\"ai-switch-ok\"}}]}",
    response_text: "ai-switch-ok",
    error_message: null,
    success: true,
    duration_ms: 321,
    stats: statsFixture({
      member_count: 1,
      request_count: 1,
      token_count: 8,
      cost_micros: 42,
    }),
    ...overrides,
  };
}
```

In `beforeEach`, reset and mock the function:

```ts
vi.mocked(routePoolTestModel).mockReset();
vi.mocked(routePoolTestModel).mockResolvedValue(modelTestOutcomeFixture());
```

Replace the existing `it("starts proxy, writes configs, and tests the credential pool route", ...)`
with this test, which verifies that route testing does not depend on starting the proxy or writing config files:

```ts
it("tests the credential pool route through the internal model connectivity check", async () => {
  renderScreen();

  expect(await screen.findByText("本地代理：未启动")).toBeInTheDocument();
  expect(screen.getByLabelText("测试算力池路由")).toBeDisabled();

  await userEvent.click(screen.getByLabelText("将 Team Account 加入算力池"));

  expect(screen.getByLabelText("测试算力池路由")).toBeEnabled();
  await userEvent.click(screen.getByLabelText("测试算力池路由"));

  await waitFor(() =>
    expect(routePoolTestModel).toHaveBeenCalledWith({
      platform: "codex",
    }),
  );
  expect(startRouteProxy).not.toHaveBeenCalled();
  expect(writeRouteProxyConfigs).not.toHaveBeenCalled();
  expect(await screen.findByText("模型连通性：通过")).toBeInTheDocument();
  expect(screen.getByText("模型输出")).toBeInTheDocument();
  expect(screen.getByText("ai-switch-ok")).toBeInTheDocument();
  expect(screen.getByText("HTTP 200 · 321 ms")).toBeInTheDocument();
  expect(screen.getByText("/chat/completions")).toBeInTheDocument();
  expect(screen.getByText(/Reply with exactly: ai-switch-ok/)).toBeInTheDocument();
  expect(screen.getByText(/choices/)).toBeInTheDocument();
  expect(screen.getByText("最近路由到：Team Account")).toBeInTheDocument();
});
```

The existing `it("clears route config write results after a short delay", ...)` test already
covers starting the proxy and writing route config files, so do not add proxy/config assertions
to the internal route-test test.

Add a new failure test after that test:

```ts
it("shows model connectivity failure details from the route test", async () => {
  vi.mocked(routePoolTestModel).mockResolvedValue(
    modelTestOutcomeFixture({
      response_status: 401,
      response_body: "{\"error\":{\"message\":\"bad key\"}}",
      response_text: null,
      success: false,
      duration_ms: 88,
    }),
  );

  renderScreen();

  await userEvent.click(await screen.findByLabelText("将 Team Account 加入算力池"));
  await userEvent.click(screen.getByLabelText("测试算力池路由"));

  expect(await screen.findByText("模型连通性：失败")).toBeInTheDocument();
  expect(screen.getByText("HTTP 401 · 88 ms")).toBeInTheDocument();
  expect(screen.getByText(/bad key/)).toBeInTheDocument();
  expect(screen.getByText("Team Account")).toBeInTheDocument();
});
```

In the request statistics test fixture, replace the `request-success` row's `metadata_json` with:

```ts
metadata_json: JSON.stringify({
  source: "ui_model_connectivity_test",
  request_kind: "model_connectivity",
  platform: "codex",
  route_credential_id: "cred-official-1",
  route_credential_name: "Team Account",
  interface_format: "openai",
  path: "/chat/completions",
  status: 200,
  success: true,
  duration_ms: 321,
  request_body_json: "{\"model\":\"gpt-5\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply with exactly: ai-switch-ok\"}]}",
  response_body: "{\"choices\":[{\"message\":{\"content\":\"ai-switch-ok\"}}]}",
  response_text: "ai-switch-ok",
  error_message: null,
}),
```

Add assertions after expanding that request detail:

```ts
expect(within(successDetail).getByText(/model_connectivity/)).toBeInTheDocument();
expect(within(successDetail).getByText(/request_body_json/)).toBeInTheDocument();
expect(within(successDetail).getByText(/response_body/)).toBeInTheDocument();
expect(within(successDetail).getByText(/ai-switch-ok/)).toBeInTheDocument();
```

- [ ] **Step 4: Run the focused frontend test and verify it fails before UI implementation**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: FAIL because `routePoolTestModel` is not imported/used by `AccountsScreen`, and the result card copy is not rendered.

### Task 4: Frontend UI Implementation

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes: `routePoolTestModel`, `RoutePoolModelTestOutcome`, and existing route pool query invalidation.
- Produces: compact "测试路由" button backed by real model test, result card showing model input/output, and request statistics refresh.

- [ ] **Step 1: Replace route-once imports and state with model-test API**

In `src/screens/AccountsScreen.tsx`, replace `routePoolRouteOnce` in the API import with `routePoolTestModel`.

In the type import block, add `RoutePoolModelTestOutcome`.

Replace:

```ts
const [lastRouteAccount, setLastRouteAccount] = useState<string | null>(null);
```

with:

```ts
const [lastRouteAccount, setLastRouteAccount] = useState<string | null>(null);
const [modelTestOutcome, setModelTestOutcome] = useState<RoutePoolModelTestOutcome | null>(null);
```

- [ ] **Step 2: Replace the mutation and click handler**

Replace `routeOnceMutation` with:

```ts
const modelTestMutation = useMutation({
  mutationFn: (request: { platform: string }) => routePoolTestModel(request),
  onSuccess: (outcome) => {
    setModelTestOutcome(outcome);
    setLastRouteAccount(outcome.selected_account_name);
    setStatsOpen(true);
    queryClient.setQueryData(["route-pool", activePlatform, statsSince, requestPage, routeStatsPageSize], {
      platform: outcome.platform,
      account_ids: routePoolQuery.data?.account_ids ?? Array.from(draftPoolIds),
      stats: outcome.stats,
    });
    void queryClient.invalidateQueries({ queryKey: ["route-pool", activePlatform] });
  },
});
```

Replace `testRoute` with:

```ts
const testRoute = () => {
  modelTestMutation.mutate({
    platform: activePlatform,
  });
};
```

Update the button disabled state:

```tsx
disabled={draftPoolIds.size === 0 || modelTestMutation.isPending}
```

Do not include `routeProxyQuery.data?.running`, `writeConfigsMutation.isPending`, or
`configWriteOutcomes.length` in this disabled condition.

Keep the visible button text as:

```tsx
测试路由
```

- [ ] **Step 3: Add result formatting helpers**

Add these helpers before `export function AccountsScreen(...)`:

```tsx
function modelTestStatusLine(outcome: RoutePoolModelTestOutcome) {
  const status = outcome.response_status ? `HTTP ${outcome.response_status}` : "无 HTTP 状态";
  return `${status} · ${outcome.duration_ms} ms`;
}

function prettyJsonOrText(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
```

- [ ] **Step 4: Render the model connectivity result card**

After the existing `configWriteOutcomes` block and before `{statsOpen && (...)}`, add:

```tsx
{modelTestOutcome ? (
  <div
    aria-label="模型连通性测试结果"
    className={`mx-4 mb-3 space-y-3 rounded-xl border px-3 py-2 text-[12px] ${
      modelTestOutcome.success
        ? "border-emerald-200 bg-emerald-50 text-emerald-950"
        : "border-red-200 bg-red-50 text-red-950"
    }`}
  >
    <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
      <div>
        <p className="font-semibold">
          模型连通性：{modelTestOutcome.success ? "通过" : "失败"}
        </p>
        <p className="text-[11px] opacity-80">
          {modelTestOutcome.selected_account_name} · {modelTestOutcome.interface_format} · {modelTestOutcome.request_path}
        </p>
      </div>
      <p className="font-mono text-[11px]">{modelTestStatusLine(modelTestOutcome)}</p>
    </div>

    {modelTestOutcome.response_text ? (
      <div>
        <p className="font-semibold">模型输出</p>
        <p className="mt-1 rounded-lg bg-white/80 px-2 py-1 font-mono text-[11px] text-stone-800">
          {modelTestOutcome.response_text}
        </p>
      </div>
    ) : null}

    {modelTestOutcome.error_message ? (
      <p className="rounded-lg bg-white/80 px-2 py-1 font-mono text-[11px] text-red-800">
        {modelTestOutcome.error_message}
      </p>
    ) : null}

    <details className="rounded-lg bg-white/80 px-2 py-1">
      <summary className="cursor-pointer font-semibold">查看输入输出</summary>
      <div className="mt-2 grid gap-2 lg:grid-cols-2">
        <div>
          <p className="mb-1 font-semibold text-stone-600">请求 JSON</p>
          <pre className="max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
            {prettyJsonOrText(modelTestOutcome.request_body_json)}
          </pre>
        </div>
        <div>
          <p className="mb-1 font-semibold text-stone-600">响应 Body</p>
          <pre className="max-h-56 overflow-auto rounded-lg border border-stone-200 bg-white p-2 font-mono text-[11px] leading-relaxed text-stone-700">
            {prettyJsonOrText(modelTestOutcome.response_body)}
          </pre>
        </div>
      </div>
    </details>
  </div>
) : null}
```

After the result card, render command-level errors:

```tsx
{modelTestMutation.isError ? (
  <div className="mx-4 mb-3 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-800">
    模型连通性测试失败：{modelTestMutation.error instanceof Error ? modelTestMutation.error.message : "请检查算力池账号和网络。"}
  </div>
) : null}
```

- [ ] **Step 5: Run focused frontend tests**

Run:

```powershell
pnpm test:run -- tests/AccountsScreen.test.tsx
```

Expected: PASS.

### Task 5: Final Verification And Commit

**Files:**
- Modify: all files from Tasks 1-4
- Modify: `docs/superpowers/plans/2026-07-19-route-model-connectivity-test.md`

**Interfaces:**
- Consumes: completed backend and frontend implementation.
- Produces: clean verification and commit.

- [ ] **Step 1: Run targeted backend checks**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml route_model_test_service
cargo test --manifest-path src-tauri/Cargo.toml route_proxy_service
cargo test --manifest-path src-tauri/Cargo.toml route_pool_service
```

Expected: all PASS.

- [ ] **Step 2: Run full backend and frontend verification**

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm typecheck
pnpm test:run
```

Expected: all PASS. Existing `ocrad.js` asm.js warnings in frontend tests are acceptable if exit code is 0.

- [ ] **Step 3: Verify the diff avoids unrelated dirty files and secrets**

Run:

```powershell
git diff --check
git diff -- src-tauri/src/services/route_model_test_service.rs src-tauri/src/models/route_pool.rs src-tauri/src/commands/route_pool_commands.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx docs/superpowers/plans/2026-07-19-route-model-connectivity-test.md
```

Expected: no whitespace errors; diff contains no API keys, access tokens, refresh tokens, `Authorization` header values, or `x-api-key` values in stored metadata.

- [ ] **Step 4: Commit only the route connectivity files**

Run:

```powershell
git add -- src-tauri/src/services/route_model_test_service.rs src-tauri/src/services/mod.rs src-tauri/src/models/route_pool.rs src-tauri/src/commands/route_pool_commands.rs src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs src/lib/api/types.ts src/lib/api/client.ts src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx docs/superpowers/plans/2026-07-19-route-model-connectivity-test.md
git commit -m "feat: test route model connectivity"
```

Expected: a commit on `main` that excludes unrelated dirty Vibe/skin files and temp screenshots.
