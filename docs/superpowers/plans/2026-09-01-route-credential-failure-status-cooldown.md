# 账号临时失败状态与可配置冷却实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**目标：**让账号在临时联系请求失败时显示可恢复的 `错误 N 次` 标签，并将失败冷却改为默认 10 秒且可按账号配置的固定时长。

**架构：**在 Rust 失败策略模型中增加 `cooldown_seconds`，由仓储层读取并用于每次临时失败的固定冷却；保留现有成功清理路径。React 账号编辑表单读写同一配置，并依据 `transient_failure_count` 选择临时失败标签或原生命周期状态标签。

**技术栈：**Tauri 2、Rust、SQLite/sqlx、React、TypeScript、Vitest。

**规格：**`docs/superpowers/specs/2026-09-01-route-credential-failure-status-cooldown-design.md`

## 全局约束

- 直接在 `main` 工作，不创建分支或 worktree。
- Rust 调试、测试和验证统一使用 `src-tauri/target-codex/`，不得创建其他构建目录。
- 失败策略中的 `retry_interval_ms` 继续表示请求内部重试间隔；`cooldown_seconds` 独立表示账号冷却。
- 用户界面使用中文文案 `错误 N 次`。
- 本次不推送，也不自动提交代码。

---

### 任务 1：先补充 Rust 失败策略与仓储行为测试

**文件：**
- 修改：`src-tauri/src/models/route_credential.rs`
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`

- [x] 写默认值、显式配置和边界校验测试。
- [x] 修改仓储测试，验证每次失败都使用配置的固定秒数，并验证成功清理。
- [x] 使用 `CARGO_TARGET_DIR=target-codex cargo test ... --lib` 确认新测试在实现前因旧逻辑失败。

### 任务 2：实现 Rust 配置字段和固定冷却

**文件：**
- 修改：`src-tauri/src/models/route_credential.rs`
- 修改：`src-tauri/src/database/repositories/route_credential_repository.rs`

- [x] 增加默认值 10 秒和安全上限。
- [x] 将字段加入默认、serde 解析和校验。
- [x] 用配置值替换渐进式 30/120/600 秒与抖动，并保持关闭开关时不生成时间字段。
- [x] 运行格式化和聚焦 Rust 测试。

### 任务 3：更新前端类型与失败策略表单

**文件：**
- 修改：`src/lib/api/types.ts`
- 修改：`src/screens/AccountsScreen.tsx`

- [x] 给 TypeScript 策略类型和默认策略加入 `cooldown_seconds: 10`。
- [x] 在配置读取、编辑初始化、保存 JSON、输入校验中贯通该字段。
- [x] 在“失败处理策略”中增加“失败冷却（秒）”输入，并更新说明文字。
- [x] 运行 `pnpm typecheck`。

### 任务 4：更新状态标签并补充前端回归测试

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`
- 修改：`tests/AccountsScreen.test.tsx`

- [x] 临时失败次数大于零时显示 `错误 N 次`。
- [x] 对明确永久/暂停状态保留原状态标签；计数清零时恢复正常标签。
- [x] 为状态标签和失败策略保存增加行为测试，使用完整账号 fixture。
- [x] 运行目标 Vitest 测试和全量 typecheck。

### 任务 5：最终验证

- [x] 运行 `cargo fmt -- --check`。
- [x] 运行聚焦仓储/代理 Rust 测试，均使用 `target-codex`。
- [x] 运行 `pnpm typecheck` 和相关 Vitest 测试。
- [x] 运行 `git diff --check`，确认 `git status --short` 只包含本次预期文件。
- [x] 向用户汇报测试结果；不推送、不提交，除非用户另行要求。
