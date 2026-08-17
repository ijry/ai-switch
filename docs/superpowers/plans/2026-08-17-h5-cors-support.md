# H5 Cross-Origin Web API Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the separate H5 mobile client issue cross-origin health and authenticated Web API requests to AI Switch without browser preflight failures.

**Architecture:** Add `tower-http`'s `CorsLayer` as the outermost layer of the existing Axum router. It will answer preflight requests before nested API authentication, emit a compatibility-focused any-origin policy on browser requests, and leave the existing Bearer-token and sensitive-command gates unchanged for real API calls.

**Tech Stack:** Rust 2021, Axum 0.7, `tower-http` 0.6 CORS middleware, Tokio, Reqwest router integration tests.

## Global Constraints

- Return `Access-Control-Allow-Origin: *`; do not add origin configuration or a UI in this change.
- Restrict the advertised methods to `GET`, `POST`, and `OPTIONS`.
- Restrict the advertised request headers to `Authorization` and `Content-Type`.
- Do not set `Access-Control-Allow-Credentials`; authentication remains explicit Bearer-token authentication.
- Apply CORS outside the nested `/api` middleware so unauthenticated `OPTIONS` preflights do not reach `authorize_api_request`.
- Actual `/api/:command` calls must retain the existing authorization, sensitive-command gating, body-limit, and no-cache behavior.
- Do not change the separate `ai-switch-app` client protocol or WebSocket origin handling.

---

### Task 1: Add router-wide CORS middleware with regression coverage

**Files:**
- Modify: `src-tauri/Cargo.toml` (add the direct `tower-http` dependency with the `cors` feature)
- Modify: `src-tauri/Cargo.lock` (record `tower-http` as a direct `ai-switch` dependency after Cargo resolves the manifest)
- Modify: `src-tauri/src/web/router.rs:5-12` (import Axum HTTP `Method` and `tower_http::cors::{Any, CorsLayer}`)
- Modify: `src-tauri/src/web/router.rs:74-80` (wrap the fully assembled router with the CORS layer after `with_state`)
- Modify: `src-tauri/src/web/router.rs:257-535` (add CORS test assertions and three router integration tests)

**Interfaces:**
- Consumes: `build_router_with_sensitive_command_gate(state, token, static_dir, sensitive_command_gate) -> Router`, already used by the in-module TCP test harness.
- Produces: private `h5_cors_layer() -> CorsLayer` that implements the confirmed policy and is applied by `build_router_with_sensitive_command_gate`.
- Preserves: `authorize_api_request`, `gate_sensitive_commands`, `disable_api_caching`, `api_command`, and every existing externally visible route path.

- [ ] **Step 1: Add failing CORS regression tests to `src-tauri/src/web/router.rs`**

  In the existing `#[cfg(test)] mod tests` module, add the two helpers below immediately after `assert_sensitive_cache_headers`. They deliberately distinguish CORS headers that must appear on all browser requests from preflight-only allow-method/header values.

  ```rust
  fn assert_h5_cors_origin(response: &reqwest::Response) {
      assert_eq!(
          response
              .headers()
              .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
              .and_then(|value| value.to_str().ok()),
          Some("*")
      );
      assert!(
          response
              .headers()
              .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
              .is_none()
      );
  }

  fn assert_h5_preflight_headers(response: &reqwest::Response) {
      assert_h5_cors_origin(response);
      let allowed_methods = response
          .headers()
          .get(header::ACCESS_CONTROL_ALLOW_METHODS)
          .and_then(|value| value.to_str().ok())
          .unwrap();
      for expected in ["GET", "POST", "OPTIONS"] {
          assert!(
              allowed_methods
                  .split(',')
                  .map(str::trim)
                  .any(|value| value == expected),
              "missing allowed method {expected}: {allowed_methods}"
          );
      }

      let allowed_headers = response
          .headers()
          .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
          .and_then(|value| value.to_str().ok())
          .unwrap()
          .to_ascii_lowercase();
      for expected in ["authorization", "content-type"] {
          assert!(
              allowed_headers
                  .split(',')
                  .map(str::trim)
                  .any(|value| value == expected),
              "missing allowed header {expected}: {allowed_headers}"
          );
      }
  }
  ```

  Add the following tests before the existing authorization tests. Each test must terminate the spawned server with `server.abort()`.

  ```rust
  #[tokio::test]
  async fn h5_preflight_bypasses_api_auth_and_advertises_supported_request_shape() {
      let (address, server) = spawn_test_router(true).await;
      let response = reqwest::Client::new()
          .request(
              reqwest::Method::OPTIONS,
              format!("http://{address}/api/list_platform_capabilities"),
          )
          .header(header::ORIGIN, "https://h5.example.test")
          .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
          .header(
              header::ACCESS_CONTROL_REQUEST_HEADERS,
              "authorization, content-type",
          )
          .send()
          .await
          .unwrap();

      assert_eq!(response.status(), StatusCode::OK);
      assert_h5_preflight_headers(&response);
      server.abort();
  }

  #[tokio::test]
  async fn h5_origin_does_not_bypass_api_bearer_authorization() {
      let (address, server) = spawn_test_router(true).await;
      let response = reqwest::Client::new()
          .post(format!("http://{address}/api/list_platform_capabilities"))
          .header(header::ORIGIN, "https://h5.example.test")
          .json(&json!({}))
          .send()
          .await
          .unwrap();

      assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
      assert_h5_cors_origin(&response);
      assert_sensitive_cache_headers(&response);
      server.abort();
  }

  #[tokio::test]
  async fn authenticated_h5_api_request_returns_cors_origin_header() {
      let (address, server) = spawn_test_router(true).await;
      let response = reqwest::Client::new()
          .post(format!("http://{address}/api/list_platform_capabilities"))
          .bearer_auth("secret")
          .header(header::ORIGIN, "https://h5.example.test")
          .json(&json!({}))
          .send()
          .await
          .unwrap();

      assert_eq!(response.status(), StatusCode::OK);
      assert_h5_cors_origin(&response);
      server.abort();
  }
  ```

- [ ] **Step 2: Run the new regression tests and confirm the current failure**

  Run:

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml h5_ -- --nocapture
  ```

  Expected before implementation: the preflight test receives `405 Method Not Allowed` rather than `200 OK`; the actual-request tests do not find `Access-Control-Allow-Origin: *`.

- [ ] **Step 3: Add the direct CORS dependency**

  In `src-tauri/Cargo.toml`, add this line after the existing `axum` dependency:

  ```toml
  tower-http = { version = "0.6", features = ["cors"] }
  ```

  Do not manually edit package checksums. Let the next Cargo command update `src-tauri/Cargo.lock`, including `tower-http` in the root `ai-switch` package dependency list.

- [ ] **Step 4: Implement a focused CORS-layer factory and apply it outside all routes**

  In `src-tauri/src/web/router.rs`, update the imports and add this private function immediately below `SENSITIVE_COMMAND_BODY_LIMIT`:

  ```rust
  use axum::http::{header, Method, StatusCode, Uri};
  use tower_http::cors::{Any, CorsLayer};

  fn h5_cors_layer() -> CorsLayer {
      CorsLayer::new()
          .allow_origin(Any)
          .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
          .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
  }
  ```

  Then apply the helper after all routes have been assembled and state has been supplied:

  ```rust
  Router::new()
      .route("/health", get(health))
      .route("/ws/events", get(events_socket))
      .nest("/api", api_router)
      .fallback(static_fallback)
      .with_state(context)
      .layer(h5_cors_layer())
  ```

  This ordering is required: the CORS layer wraps `api_router`, so an OPTIONS preflight is completed before `authorize_api_request`, while a POST still flows through all existing API middleware.

- [ ] **Step 5: Format and run the targeted regression suite**

  Run:

  ```bash
  cargo fmt --manifest-path src-tauri/Cargo.toml --check
  cargo test --manifest-path src-tauri/Cargo.toml h5_ -- --nocapture
  ```

  Expected: formatting passes and all three H5 CORS tests pass. The preflight returns `200 OK`, exposes `*`, advertises the three methods and two request headers, and does not expose `Access-Control-Allow-Credentials`. The missing-token POST remains `401 Unauthorized`; the authenticated POST remains `200 OK`.

- [ ] **Step 6: Run the surrounding router regression suite**

  Run:

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml web::router::tests -- --nocapture
  ```

  Expected: all existing router authorization, sensitive-command gate, no-cache, and body-limit tests continue to pass alongside the three new CORS tests.

- [ ] **Step 7: Verify the running Web Service after it has been restarted with the rebuilt binary**

  Use a browser-equivalent, read-only preflight request. No Bearer token is required for this OPTIONS request:

  ```bash
  curl --silent --show-error --include --request OPTIONS \
    http://127.0.0.1:13090/api/list_platform_capabilities \
    --header 'Origin: http://localhost:5173' \
    --header 'Access-Control-Request-Method: POST' \
    --header 'Access-Control-Request-Headers: authorization, content-type'
  ```

  Expected response headers include `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods` containing `GET`, `POST`, and `OPTIONS`, and `Access-Control-Allow-Headers` containing `authorization` and `content-type`. The response must not include `Access-Control-Allow-Credentials`.

  Then validate that normal API authentication remains required, keeping the token out of shell history by supplying it from the environment:

  ```bash
  : "${AI_SWITCH_TOKEN:?Set AI_SWITCH_TOKEN before running this check}"
  curl --silent --show-error --fail-with-body \
    http://127.0.0.1:13090/api/list_platform_capabilities \
    --request POST \
    --header "Authorization: Bearer ${AI_SWITCH_TOKEN}" \
    --header 'Content-Type: application/json' \
    --header 'Origin: http://localhost:5173' \
    --data '{}'
  ```

  Expected: the authenticated POST returns JSON successfully and includes `Access-Control-Allow-Origin: *`.

- [ ] **Step 8: Review the final diff and commit the feature**

  Run:

  ```bash
  git diff --check
  git diff -- src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/web/router.rs
  git status --short
  ```

  Stage only the CORS implementation files and create a focused commit:

  ```bash
  git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/web/router.rs
  git commit -m "feat: 支持 H5 跨域访问 Web API"
  ```
