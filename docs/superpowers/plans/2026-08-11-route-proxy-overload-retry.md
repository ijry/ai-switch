# 路由代理上游过载自动重试 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** 对 `Our servers are currently overloaded` 语义失败执行每账号 5 次额外重试，成功时隐藏中间错误，失败时保持账号业务状态不变。

**Architecture:** 保留现有账号池顺序和单次请求 `forward_request` 主循环，将凭证索引改为带“过载重试次数”的队列。过载失败只把同一账号以递增计数重新放回队首，其他错误不重新入队。使用固定退避序列，并继续复用现有临时失败记录、请求统计和实时日志。

**Tech Stack:** Rust 2021、Axum、Reqwest、Tokio、SQLx SQLite、现有 route proxy 单元/集成测试。

## Global Constraints

- 仅识别 `Our servers are currently overloaded` 过载语义失败，不扩大到全部 `response.failed`。
- 每个账号每次客户端请求最多额外重试 5 次，即单个账号最多尝试 6 次。
- 账号之间按现有池顺序切换，每个账号独立计算 5 次预算。
- 过载重试成功时客户端只接收成功响应；失败时账号不得被设为 `error`、`paused` 或 `revoked`。
- 固定窗口额度耗尽、非过载语义失败和永久认证错误继续使用现有状态规则。
- 默认退避序列为 300ms、1s、2s、3s、5s；不新增前端配置。

---

### Task 1: 为请求级过载重试定义队列和退避策略

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:35-80`
- Test: `src-tauri/src/services/route_proxy_service.rs` 的 `#[cfg(test)]` 模块

**Interfaces:**
- Produces `OVERLOAD_MAX_EXTRA_RETRIES: usize = 5`、`overload_retry_delay(retry_count: usize) -> Duration`，供 `forward_request` 使用。
- 队列元素使用 `(credential_index: usize, overload_retry_count: usize)`，其中 `0` 表示该账号尚未进行额外重试。

- [ ] **Step 1: 写退避策略失败测试**

在 route proxy 测试模块添加：

```rust
#[test]
fn overload_retry_delay_uses_bounded_backoff_sequence() {
    assert_eq!(overload_retry_delay(0), Duration::from_millis(300));
    assert_eq!(overload_retry_delay(1), Duration::from_secs(1));
    assert_eq!(overload_retry_delay(2), Duration::from_secs(2));
    assert_eq!(overload_retry_delay(3), Duration::from_secs(3));
    assert_eq!(overload_retry_delay(4), Duration::from_secs(5));
    assert_eq!(overload_retry_delay(5), Duration::from_secs(5));
}
```

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
cargo test --lib overload_retry_delay_uses_bounded_backoff_sequence `
  --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：编译失败，提示 `overload_retry_delay` 尚未定义。

- [ ] **Step 3: 实现常量和退避函数**

在现有 route proxy 常量附近加入：

```rust
const OVERLOAD_MAX_EXTRA_RETRIES: usize = 5;
const OVERLOAD_RETRY_DELAYS: [Duration; OVERLOAD_MAX_EXTRA_RETRIES] = [
    Duration::from_millis(300),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(3),
    Duration::from_secs(5),
];

fn overload_retry_delay(retry_count: usize) -> Duration {
    OVERLOAD_RETRY_DELAYS
        .get(retry_count)
        .copied()
        .unwrap_or_else(|| *OVERLOAD_RETRY_DELAYS.last().expect("retry delays are non-empty"))
}
```

如果编译器不接受常量数组中的 `Duration` 构造，则改为毫秒数组并在函数中调用 `Duration::from_millis`，保持相同的公开行为和测试断言。

- [ ] **Step 4: 运行测试确认通过**

运行同一条聚焦命令，预期 1 个测试通过。

- [ ] **Step 5: 提交**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: add overload retry backoff policy"
```

### Task 2: 将账号池循环改为支持同账号重试的队列

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:40-110`（导入）和 `forward_request`
- Test: `src-tauri/src/services/route_proxy_service.rs` 的 route proxy 集成测试

**Interfaces:**
- Consumes `overload_retry_delay` 和 `OVERLOAD_MAX_EXTRA_RETRIES`。
- Produces请求级 `VecDeque<(usize, usize)>`，初始内容为现有 `credential_indexes_by_priority` 的每个索引各配 `0` 次重试。

- [ ] **Step 1: 写“第 6 次成功且客户端无错误”失败测试**

在测试模块新增带序列响应的上游 fixture。使用 `Arc<AtomicUsize>` 计数请求次数：前 5 次返回 HTTP 200 和 `response.failed` 过载消息，第 6 次返回普通成功 JSON。测试断言：

```rust
assert_eq!(response.status(), StatusCode::OK);
assert_eq!(response.json::<Value>().await.expect("body")["ok"], true);
assert_eq!(calls.load(Ordering::SeqCst), 6);
assert_eq!(
    RouteCredentialRepository::get(&pool, &credential_id)
        .await
        .expect("credential")
        .status,
    "ok"
);
```

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
cargo test --lib retries_overloaded_account_until_success `
  --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：当前代理只请求 1 次，测试在调用次数或成功状态断言处失败。

- [ ] **Step 3: 引入 `VecDeque` 请求队列**

将 `std::collections` 导入扩展为：

```rust
use std::collections::{BTreeMap, HashMap, VecDeque};
```

将现有：

```rust
let retry_indexes = credential_indexes_by_priority(&credentials, cursor);
for (attempt, credential_index) in retry_indexes.into_iter().enumerate() {
```

改为：

```rust
let retry_indexes = credential_indexes_by_priority(&credentials, cursor);
let mut retry_queue = retry_indexes
    .into_iter()
    .map(|credential_index| (credential_index, 0usize))
    .collect::<VecDeque<_>>();
let mut attempt = 0usize;

while let Some((credential_index, overload_retry_count)) = retry_queue.pop_front() {
    attempt += 1;
```

保留每轮已有的并发租约、刷新、请求构建、响应转换、统计和日志逻辑。所有原有 `continue` 都继续表示“结束当前队列项并处理下一个账号/队列项”。

- [ ] **Step 4: 在过载分支中重新入队同一账号**

将语义失败分支调整为：

```rust
if let Some(failure) = semantic_failure {
    if is_transient_response_failure(&failure.message) {
        record_route_credential_failure(
            &state.activity,
            &platform,
            pool,
            &credential.id,
            "semantic_response_transient",
            &failure.message,
            Some(&response_bytes),
        )
        .await;

        if overload_retry_count < OVERLOAD_MAX_EXTRA_RETRIES {
            tokio::time::sleep(overload_retry_delay(overload_retry_count)).await;
            retry_queue.push_front((credential_index, overload_retry_count + 1));
        } else {
            retry_errors.push(format!(
                "{}: overload retries exhausted",
                credential.display_name
            ));
        }
    } else {
        if RouteCredentialRepository::record_semantic_failure(
            pool,
            &credential.id,
            &failure.message,
            Some(&response_bytes),
        )
        .await
        .is_ok()
        {
            state
                .activity
                .notify_status_change(&platform, &credential.id);
        }
        retry_errors.push(format!("{}: {}", credential.display_name, failure.message));
    }
    continue;
}
```

过载成功响应会走现有成功返回路径，并清理临时失败；其他失败不会进入同账号重试队列。

- [ ] **Step 5: 运行新增集成测试**

运行：

```powershell
cargo test --lib retries_overloaded_account_until_success `
  --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：1 个测试通过，客户端收到成功响应，账号状态仍为 `ok`。

- [ ] **Step 6: 提交**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: retry overloaded route accounts"
```

### Task 3: 覆盖重试耗尽、账号切换和现有状态规则

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs` 的测试模块
- Modify: `src-tauri/src/services/response_failure_service.rs`（仅在回归测试需要补充嵌套错误码覆盖时）
- Test: `src-tauri/src/services/route_model_test_service.rs` 现有过载测试

**Interfaces:**
- Reuses `OVERLOAD_MAX_EXTRA_RETRIES`, `overload_retry_delay` 和请求队列。
- 不改变 `is_transient_response_failure`、`detect_auto_pause_code`、`record_semantic_failure` 的外部契约。

- [ ] **Step 1: 写重试耗尽失败测试**

新增始终返回过载响应的单账号测试，断言调用次数为 6、客户端最终收到非成功响应、账号状态仍为 `ok`、`transient_failure_count` 增加。

- [ ] **Step 2: 写账号切换失败测试**

新增两个账号的序列上游：第一个账号过载 6 次，第二个账号第一次返回成功。断言第一个账号调用 6 次，第二个账号调用 1 次，客户端成功，两个账号都没有被标记为 `error`。

- [ ] **Step 3: 运行测试确认行为**

运行：

```powershell
cargo test --lib overload --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：所有过载重试相关测试通过。

- [ ] **Step 4: 运行状态回归测试**

运行：

```powershell
cargo test --lib response_failure_service::tests `
  --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
cargo test --lib test_model_overloaded_response_keeps_account_ok `
  --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：过载仍保持 `ok`，非过载语义失败、固定窗口额度耗尽和永久认证错误的既有测试不回归。

- [ ] **Step 5: 提交**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "test: cover overload retry exhaustion and account rotation"
```

### Task 4: 完整验证

**Files:**
- Verify only; no source changes expected.

- [ ] **Step 1: 运行格式和差异检查**

```powershell
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 2: 运行 Rust 全量测试**

```powershell
cargo test --lib --manifest-path src-tauri/Cargo.toml `
  --target-dir "$env:TEMP\ai-switch-rust-check-2"
```

预期：全部测试通过，新增重试测试包含在总数中。

- [ ] **Step 3: 运行前端验证**

```powershell
pnpm typecheck
pnpm test:run
```

预期：类型检查通过，前端测试全部通过。

- [ ] **Step 4: 提交最终实现**

```powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: hide transient overload errors with retries"
```
