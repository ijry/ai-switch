# 路由代理鉴权错误提示改进实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 区分“未提供本地代理鉴权信息”和“提供了无效 SK”两类 401，使调用方能直接定位配置问题。

**Architecture:** 保持现有 `resolve_platform` 的平台解析流程不变，仅记录请求是否带有可提取的鉴权凭据，并在凭据查不到且没有平台头时返回独立错误码。`json_error` 继续统一返回 401，但根据错误码输出对应的错误类型和提示。

**Tech Stack:** Rust、Axum、Sqlx、Cargo 单元测试。

## Global Constraints

- 不修改 FastView 调用方协议。
- 不记录或回显实际 SK 内容。
- 保留无鉴权请求的现有错误码兼容性。
- 不创建分支、不创建 worktree、不提交 Git commit。

---

### Task 1: 覆盖无效 SK 的失败行为

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:1161-1186`
- Test: `src-tauri/src/services/route_proxy_service.rs` 内 `resolve_platform` 测试模块

**Interfaces:**
- Consumes: `resolve_platform(&ProxyAppState, &HeaderMap, Option<&str>)`.
- Produces: `AppError::Validation { code: "route_proxy.key_invalid", ... }`，仅用于带有不可解析本地代理 key 且没有显式平台头的请求。

- [x] **Step 1: 添加无效 Bearer SK 测试**

构造内存数据库和 `ProxyAppState`，向 `HeaderMap` 写入 `Authorization: Bearer sk-invalid`，调用 `resolve_platform`，断言错误码为 `route_proxy.key_invalid`，并断言消息不包含实际 key。

- [x] **Step 2: 运行目标测试确认当前实现失败**

Run: `cargo test route_proxy_service::tests::resolve_platform_rejects_unknown_proxy_key --lib`

Expected: 当前实现因仍返回 `route_proxy.platform_unresolved` 而失败。

- [x] **Step 3: 调整解析分支**

在 `resolve_platform` 中保留现有 key 查找；当 `inbound_key.is_some()`、key 查找失败且没有 `x-ai-switch-platform` 时，返回 `route_proxy.key_invalid`，提示 SK 可能无效、已更换或来自其他 ai-switch 实例。

- [x] **Step 4: 运行目标测试确认通过**

Run: `cargo test route_proxy_service::tests::resolve_platform_rejects_unknown_proxy_key --lib`

Expected: PASS。

### Task 2: 保留无鉴权请求并统一改善响应文本

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs:1125-1159`
- Test: `src-tauri/src/services/route_proxy_service.rs` 内 `resolve_platform` 和错误响应测试

**Interfaces:**
- Consumes: `route_proxy.platform_unresolved` 与 `route_proxy.key_invalid` 错误消息。
- Produces: 两类错误都返回 HTTP 401；响应 JSON 的 `error.code` 分别为 `route_proxy.auth_required` 和 `route_proxy.key_invalid`。

- [x] **Step 1: 添加错误响应映射断言**

覆盖无鉴权错误仍映射为 `route_proxy.auth_required`，无效 SK 错误映射为 `route_proxy.key_invalid`，两者均设置 `WWW-Authenticate: Bearer`。

- [x] **Step 2: 更新 `json_error` 的错误码识别**

优先根据错误消息中的 `route_proxy.key_invalid` 返回同名响应码；保留 `route_proxy.platform_unresolved` 到 `route_proxy.auth_required` 的兼容行为；避免把无效 SK 再显示成“平台无法解析”。

- [x] **Step 3: 运行服务端目标测试**

Run: `cargo test route_proxy_service::tests --lib`

Expected: PASS；若存在与本改动无关的已有失败，仅报告其原始失败信息。

### Task 3: 完成静态检查并复核差异

**Files:**
- Verify: `src-tauri/src/services/route_proxy_service.rs`
- Verify: `docs/superpowers/plans/2026-08-09-route-proxy-auth-error.md`

- [x] **Step 1: 检查格式**

Run: `cargo fmt --all -- --check`

Expected: PASS。

- [x] **Step 2: 检查工作区差异**

Run: `git diff --check` and `git status --short`

Expected: 只有本次错误提示改动和计划文档；不覆盖用户已有的 `src-tauri/src/services/route_model_fetch_service.rs` 修改。
