# 用量总览合并 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把本机 CLI 会话记录与代理数据库记录合并成一个去重后的列表与一套「我的总花费」数字，并把按模型等维度的明细收进默认折叠的分段器。

**Architecture:** 合并、去重、分组、分页全在 Rust 侧完成（会话语料可达 6 万余行，无法搬到前端）。去重键是上游响应 id：新代理行在写入时抽取并存入新列 `upstream_response_id`，历史行回退到解析 `response_body` 预览。Codex 会话由「只取文件最后一条累计值」改为对相邻累计值做差得到逐轮记录。前端新增独立面板组件，只消费一个新命令。

**Tech Stack:** Rust（Tauri 2、sqlx/SQLite、serde、tokio）、TypeScript + React 18、TanStack Query v5、Vitest + Testing Library。

**Spec:** `docs/superpowers/specs/2026-09-02-usage-overview-merge-design.md`

## Global Constraints

- 工作目录：`D:\Repos\xyito\open\ai-switch-task-14`，当前分支 `task/14`。只在本分支提交，**不要** merge / rebase / push 到 `main`。
- AI 执行 cargo 必须复用 `target-codex`：在 `src-tauri` 目录下用 `CARGO_TARGET_DIR=target-codex cargo ...`。禁止新建其他 target 目录（`AGENTS.md:6-10`）。
- `cargo` 前置：`src-tauri/build.rs` 会校验 `src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe` 与仓库根 `dist/` 存在，缺任一个都报 `resource path ... doesn't exist`（看起来像编译错误但与代码无关）。本 worktree 两者**都缺**：sidecar 构建见 `src-tauri/binaries/README.md`（在 `sidecar/ai-switch-tsnet` 里 `go build`），`dist/` 可用 `pnpm build` 生成，或直接建一个空目录让 build script 通过。两者都在 .gitignore 内，建好后不要删。
- 前端测试：`pnpm test:run`（Vitest）。类型检查：`pnpm typecheck`。本 worktree 缺 `node_modules`，需先 `pnpm install`。
- **绝对不要修改已发布的迁移文件**。sqlx 按字节哈希校验 `.sql`，改动会让所有用户的库被隔离（v0.7.3 的行尾归一化清空过全部用户账号列表）。只新增迁移。
- 新迁移文件名固定为 `202609020003_usage_upstream_response_id.sql`（`202609020001` 与 `202609020002` 已被 main 分支的 `route_credential_external_source` 与 `route_credential_models` 占用——版本号是 sqlx 的主键，撞号会让用户的库被隔离）。
- 金额单位：USD micros（1 USD = 1_000_000）。CNY→USD 固定汇率 `model_pricing::CNY_PER_USD = 7.1`。
- 界面文案用简体中文，与现有统计面板一致。`AccountsScreen` 未接 i18n，新面板同样用硬编码中文。
- 数字单位严格三档：≥10_000 用「万」（1 位小数），≥1_000_000 用「百万」（2 位小数），≥100_000_000 用「亿」（2 位小数），小于 10_000 原样带千分位。档位由**原值**决定，不因小数进位跨档。
- 费用不套万/百万/亿，沿用现有货币格式（`formatCostMicros`）。

---

## 文件结构

**新建**

| 文件 | 职责 |
| --- | --- |
| `src-tauri/migrations/202609020003_usage_upstream_response_id.sql` | 加 `upstream_response_id` 列与索引 |
| `src-tauri/src/services/upstream_response_id.rs` | 从响应字节抽取上游响应 id（四种形态），代理写入与历史行回退共用 |
| `src-tauri/src/services/usage_overview_service.rs` | 合并去重、汇总、四维分组、分页 |
| `src-tauri/src/core/usage_overview.rs` | Tauri 命令与 web 分发器共用的入口（沿用 `core::usage_stats` 的模式） |
| `src/lib/usageFormat.ts` | 万/百万/亿 单位格式化 |
| `src/components/accounts/UsageOverviewPanel.tsx` | 统计面板（自带 state、查询、分页） |
| `tests/lib/usageFormat.test.ts` | 单位格式化边界值 |
| `tests/UsageOverviewPanel.test.tsx` | 面板渲染、来源徽标、分段器 |

**修改**

| 文件 | 改动 |
| --- | --- |
| `src-tauri/src/services/session_usage_service.rs` | 暴露逐条记录；codex 改为逐轮拆分；抽取会话侧去重键 |
| `src-tauri/src/services/route_proxy_service.rs` | 写入时抽取并保存 `upstream_response_id` |
| `src-tauri/src/services/route_model_test_service.rs` | 同上 |
| `src-tauri/src/database/repositories/route_pool_repository.rs` | `insert_request_event` 写新列；新增按时间窗读取全部 request 行的方法 |
| `src-tauri/src/services/mod.rs`、`src-tauri/src/core/mod.rs`、`src-tauri/src/commands/usage_stats_commands.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/web/handlers/mod.rs` | 注册新模块与命令 |
| `src/lib/api/types.ts`、`src/lib/api/client.ts` | 新类型与 `getUsageOverview` |
| `src/screens/AccountsScreen.tsx` | 移出统计实现，保留视图开关 |
| `tests/AccountsScreen.test.tsx` | 移除已迁走的统计测试 |

**任务顺序**：Task 1-2 是纯函数（无依赖，可先落地并测）→ Task 3 迁移 + 写入路径 → Task 4 会话侧逐条记录 → Task 5 合并服务 → Task 6 命令注册 → Task 7 前端类型与 client → Task 8 面板组件 → Task 9 接入 AccountsScreen 并清理旧代码。

---

### Task 0: 准备构建环境

不产出代码，但后续每个 Rust 任务都依赖它。单独成任务是因为它有独立的验收标准，且失败信息容易被误判为代码问题。

**Files:** 无（只产出 .gitignore 内的构建产物）

- [ ] **Step 1: 装前端依赖**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14
pnpm install
```

- [ ] **Step 2: 生成 dist/**

```bash
pnpm build
```

若 `pnpm build` 因与本任务无关的原因失败，退而建一个空目录让 build script 通过：

```bash
mkdir -p dist
```

- [ ] **Step 3: 确认 go sidecar 二进制存在**

```bash
ls -la src-tauri/binaries/
```

期望看到 `ai-switch-tsnet-x86_64-pc-windows-msvc.exe`。若只有 `README.md`，按其说明构建：

```bash
cd sidecar/ai-switch-tsnet
go build -o ../../src-tauri/binaries/ai-switch-tsnet-x86_64-pc-windows-msvc.exe .
```

首次构建耗时可观（产物约 31 MB）。

- [ ] **Step 4: 验证 cargo 能跑起来**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo check
```

期望：编译通过（首次会拉依赖，耗时较长）。若报 `resource path ... doesn't exist`，回到 Step 2/3 检查产物，**不要**去查代码。

- [ ] **Step 5: 验证前端测试基线**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14 && pnpm test:run
```

期望：全绿。记下用时与用例数，作为后续对比基线。

无需提交（产物都在 .gitignore 内）。

---

### Task 1: 上游响应 id 抽取

纯函数，无依赖。代理写入路径（Task 3）与历史行回退解析（Task 5）都用它，所以先独立落地。

**Files:**
- Create: `src-tauri/src/services/upstream_response_id.rs`
- Modify: `src-tauri/src/services/mod.rs`（加 `pub mod upstream_response_id;`，按字母序插在 `tailscale_service` 之后、`target_service` 之前）

**Interfaces:**
- Consumes: `crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body: &[u8]) -> Vec<serde_json::Value>`（已存在）
- Produces: `pub fn extract_upstream_response_id(body: &[u8]) -> Option<String>`

**背景**（实现者需知道的事实，来自真实数据核查）：

四种响应形态各自把 id 放在不同位置：

| 形态 | id 位置 | 实例 |
| --- | --- | --- |
| Anthropic 流式 | `message_start` 帧的 `message.id` | `msg_ba2673091d85447a977690119b3b302d` |
| Anthropic 非流式 | 顶层 `id` | `msg_...` |
| OpenAI Responses 流式 | `response.created` 帧的 `response.id` | `5d76e101-2615-4e87-8455-72061b36392c` |
| OpenAI Chat Completions | 顶层 `id` | `chatcmpl-...` |

已有的 `route_proxy_service::extract_response_model`（`:4769-4776`）是同一个形状的问题——先试整体 JSON 解析，失败再逐帧扫 SSE。照它的结构写。

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/services/upstream_response_id.rs`，只写测试模块和一个空的函数签名：

```rust
//! Extract the upstream response id from a response body.
//!
//! This id is the join key between a proxied request and the CLI transcript
//! entry for the same request: Claude Code records `message.id` per assistant
//! message, and Codex embeds the Responses id in its `rs_` / `msg_` / `fc_`
//! item ids. On a real corpus the two sides matched 2905/2933 (99.0%).

use serde_json::Value;

pub fn extract_upstream_response_id(_body: &[u8]) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_anthropic_streaming_message_start() {
        let body = b"event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_abc123\",\"model\":\"claude-opus-5\",\"usage\":{\"input_tokens\":10}}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_abc123")
        );
    }

    #[test]
    fn reads_anthropic_non_streaming_top_level_id() {
        let body = br#"{"id":"msg_def456","type":"message","content":[],"usage":{"input_tokens":5}}"#;
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_def456")
        );
    }

    #[test]
    fn reads_openai_responses_created_frame() {
        let body = b"event: response.created\n\
data: {\"response\":{\"id\":\"5d76e101-2615-4e87-8455-72061b36392c\",\"object\":\"response\",\"model\":\"deepseek-v4-flash\"}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("5d76e101-2615-4e87-8455-72061b36392c")
        );
    }

    #[test]
    fn reads_chat_completions_top_level_id() {
        let body = br#"{"id":"chatcmpl-route","object":"chat.completion","choices":[]}"#;
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("chatcmpl-route")
        );
    }

    #[test]
    fn returns_none_when_no_id_is_present() {
        // A truncated preview can cut off before the id, and an error body has
        // none at all. Both must read as "unknown" rather than as a bogus key,
        // because a wrong key would merge two unrelated requests.
        assert_eq!(extract_upstream_response_id(b""), None);
        assert_eq!(extract_upstream_response_id(b"not json at all"), None);
        assert_eq!(
            extract_upstream_response_id(br#"{"error":{"message":"expired"}}"#),
            None
        );
    }

    #[test]
    fn ignores_blank_and_non_string_ids() {
        assert_eq!(extract_upstream_response_id(br#"{"id":"   "}"#), None);
        assert_eq!(extract_upstream_response_id(br#"{"id":123}"#), None);
    }

    #[test]
    fn prefers_the_first_frame_that_carries_an_id() {
        // `message_start` comes first in a real Anthropic stream; later frames
        // (content_block_delta) carry no id, so scanning must not stop at the
        // first frame unconditionally, nor overwrite with a later empty value.
        let body = b"event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_second_frame\"}}\n\n";
        assert_eq!(
            extract_upstream_response_id(body).as_deref(),
            Some("msg_second_frame")
        );
    }
}
```

在 `src-tauri/src/services/mod.rs` 加一行（按字母序，`tailscale_types` 之后、`target_service` 之前）：

```rust
pub mod upstream_response_id;
```

注意：字母序上 `upstream_response_id` 排在 `target_service` 与 `web_service` 之间，插在那里。

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib upstream_response_id
```

期望：7 个测试中 6 个 FAIL（断言 `Some(...)` 的都失败，`returns_none_when_no_id_is_present` 与 `ignores_blank_and_non_string_ids` 会误过）。

- [ ] **Step 3: 实现**

替换空函数：

```rust
pub fn extract_upstream_response_id(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return response_id_from_value(&value);
    }
    crate::services::route_protocol_bridge::sse::parse_sse_data_records_lossy(body)
        .iter()
        .find_map(response_id_from_value)
}

fn response_id_from_value(value: &Value) -> Option<String> {
    [
        value.pointer("/message/id"),
        value.pointer("/response/id"),
        value.get("id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .find(|id| !id.is_empty())
    .map(str::to_string)
}
```

顺序说明：嵌套路径先于顶层 `id`。Anthropic 的 `message_start` 帧顶层有 `"type":"message_start"` 但没有 `id`，而 OpenAI Responses 的 `response.created` 帧顶层同样无 `id`——不过若某个上游同时给出两者，响应自身的 id 比包装层的更权威。

- [ ] **Step 4: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib upstream_response_id
```

期望：7 passed。

- [ ] **Step 5: 提交**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14
git add src-tauri/src/services/upstream_response_id.rs src-tauri/src/services/mod.rs
git commit -m "feat: 抽取上游响应 id 作为会话与代理记录的关联键"
```

---

### Task 2: 万/百万/亿 单位格式化

纯前端函数，与 Rust 侧无依赖，可与 Task 1 并行。

**Files:**
- Create: `src/lib/usageFormat.ts`
- Test: `tests/lib/usageFormat.test.ts`

**Interfaces:**
- Produces:
  - `export function formatCompactCount(value: number): string`
  - `export function formatExactCount(value: number): string`

**背景**：现有 `AccountsScreen.tsx:492-500` 的 `formatTokenCount` 用英文缩写（`5.58B` / `2.1M`），要换成中文单位。三档同时存在会打断中文按 1e4 分组的进制（万 → 亿），所以列表里会并存「万」与「百万」——这是明确选定的取舍。

档位由**原值**决定，不由格式化后的数字决定：99,999,999 除以 1e6 是 99.999999，保留两位后显示 `100.00百万`——看着像该进位成 `1.00亿`，但档位在取原值时就定了。这是有意的，测试要把它钉住。

小数位用 `toFixed` 的四舍五入，与被替换的 `formatTokenCount`（`AccountsScreen.tsx:492-500`）保持一致。

- [ ] **Step 1: 写失败测试**

创建 `tests/lib/usageFormat.test.ts`：

```typescript
import { describe, expect, it } from "vitest";
import { formatCompactCount, formatExactCount } from "../../src/lib/usageFormat";

describe("formatCompactCount", () => {
  it("leaves values under 10,000 as thousands-separated digits", () => {
    expect(formatCompactCount(0)).toBe("0");
    expect(formatCompactCount(999)).toBe("999");
    expect(formatCompactCount(9_999)).toBe("9,999");
  });

  it("switches to 万 at 10,000 with one decimal", () => {
    expect(formatCompactCount(10_000)).toBe("1.0万");
    expect(formatCompactCount(25_000)).toBe("2.5万");
    expect(formatCompactCount(250_000)).toBe("25.0万");
  });

  it("switches to 百万 at 1,000,000 with two decimals", () => {
    expect(formatCompactCount(1_000_000)).toBe("1.00百万");
    expect(formatCompactCount(2_500_000)).toBe("2.50百万");
  });

  it("switches to 亿 at 100,000,000 with two decimals", () => {
    expect(formatCompactCount(100_000_000)).toBe("1.00亿");
    expect(formatCompactCount(5_584_802_591)).toBe("55.85亿");
  });

  it("picks the tier from the raw value, not from the rounded figure", () => {
    // 999,999 is below the 百万 threshold, so it stays in 万 even though the
    // rounded mantissa reads 100.0. Same one tier up: 99,999,999 renders as
    // 100.00百万 rather than jumping to 1.00亿. Both look odd and both are
    // deliberate — the tier is chosen before any rounding happens.
    expect(formatCompactCount(999_999)).toBe("100.0万");
    expect(formatCompactCount(99_999_999)).toBe("100.00百万");
  });

  it("handles negatives and non-finite input without producing garbage", () => {
    // Token counts should never be negative, but a malformed payload must not
    // render as "NaN万" in a summary card.
    expect(formatCompactCount(-1)).toBe("0");
    expect(formatCompactCount(Number.NaN)).toBe("0");
    expect(formatCompactCount(Number.POSITIVE_INFINITY)).toBe("0");
  });
});

describe("formatExactCount", () => {
  it("renders the precise figure for the tooltip", () => {
    expect(formatExactCount(5_584_802_591)).toBe("5,584,802,591");
    expect(formatExactCount(0)).toBe("0");
  });

  it("clamps invalid input the same way the compact form does", () => {
    expect(formatExactCount(-5)).toBe("0");
    expect(formatExactCount(Number.NaN)).toBe("0");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14
pnpm vitest run tests/lib/usageFormat.test.ts
```

期望：FAIL，`Failed to resolve import "../../src/lib/usageFormat"`。

- [ ] **Step 3: 实现**

创建 `src/lib/usageFormat.ts`：

```typescript
const TIERS = [
  { threshold: 100_000_000, suffix: "亿", decimals: 2 },
  { threshold: 1_000_000, suffix: "百万", decimals: 2 },
  { threshold: 10_000, suffix: "万", decimals: 1 },
] as const;

function sanitize(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/**
 * Compact a count for a summary card or a table cell.
 *
 * The tier is chosen from the raw value before any rounding, so 99,999,999
 * stays in 百万 (as "100.00百万") instead of rounding itself up into 亿.
 */
export function formatCompactCount(value: number): string {
  const safe = sanitize(value);
  const tier = TIERS.find((candidate) => safe >= candidate.threshold);
  if (!tier) {
    return safe.toLocaleString("en-US");
  }
  return `${(safe / tier.threshold).toFixed(tier.decimals)}${tier.suffix}`;
}

/** The precise figure, for the `title` tooltip beside a compact value. */
export function formatExactCount(value: number): string {
  return sanitize(value).toLocaleString("en-US");
}
```

`toLocaleString("en-US")` 而非无参调用：无参会跟随运行环境 locale，测试环境与用户环境可能给出不同的千分位符号，而现有测试 `getByTitle("5,584,802,591")` 依赖逗号。

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm vitest run tests/lib/usageFormat.test.ts
```

期望：全部 passed。

- [ ] **Step 5: 提交**

```bash
git add src/lib/usageFormat.ts tests/lib/usageFormat.test.ts
git commit -m "feat: 用量数字改用万/百万/亿单位"
```

---

### Task 3: 迁移与写入路径

加新列，并让三个写入点把上游响应 id 存进去。

**Files:**
- Create: `src-tauri/migrations/202609020003_usage_upstream_response_id.sql`
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs:332-369`（`insert_request_event`）
- Modify: `src-tauri/src/services/route_proxy_service.rs:5086-5100`（`insert_route_credential_request_event`）
- Modify: `src-tauri/src/services/route_model_test_service.rs:1372-1379`
- Modify: `src-tauri/src/models/route_pool.rs:51-70`（`RoutePoolUsageLog` 加字段）

**Interfaces:**
- Consumes: `crate::services::upstream_response_id::extract_upstream_response_id`（Task 1）
- Produces:
  - `RoutePoolRepository::insert_request_event(pool, account_id, source_label, metadata_json, usage, upstream_response_id: Option<&str>)` —— 末尾新增一个参数
  - `RoutePoolUsageLog.upstream_response_id: Option<String>`

**警告**：`insert_request_event` 有 9 处调用点（route_proxy 8 处经由 `insert_route_credential_request_event` 收敛为 1 处，route_model_test 1 处，route_pool_service 手写 SQL 不走这个函数）。改签名后编译器会指出所有需要更新的位置。

- [ ] **Step 1: 写迁移**

创建 `src-tauri/migrations/202609020003_usage_upstream_response_id.sql`：

```sql
-- The upstream response id is the join key between a proxied request and the CLI
-- transcript entry for the same request. Extracting it at write time avoids
-- re-scanning every metadata_json on each stats refresh.
--
-- NULL means unknown: a pre-migration row, a failed request that never got a
-- response, or a body preview that was truncated before the id.
ALTER TABLE usage_events ADD COLUMN upstream_response_id TEXT;

-- Rows are looked up by this id when merging, so the index carries the join.
CREATE INDEX IF NOT EXISTS idx_usage_events_upstream_response_id
  ON usage_events (upstream_response_id);
```

不做 backfill：历史行由查询时解析 `response_body` 预览兜住（Task 5），且该集合会随旧数据滑出时间窗而缩小。

- [ ] **Step 2: 写失败测试**

在 `src-tauri/src/database/repositories/route_pool_repository.rs` 的 `#[cfg(test)] mod tests`（约 `:533` 起）内新增：

```rust
    #[tokio::test]
    async fn request_event_persists_the_upstream_response_id() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "claude", "ClaudeOne").await;

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"platform":"claude","success":true}"#,
            &RouteUsageBreakdown::default(),
            Some("msg_abc123"),
        )
        .await
        .expect("insert");

        let stats = RoutePoolRepository::stats(&pool, "claude", None, 1, 20)
            .await
            .expect("stats");
        assert_eq!(
            stats.requests[0].upstream_response_id.as_deref(),
            Some("msg_abc123")
        );
    }

    #[tokio::test]
    async fn request_event_without_a_response_id_stores_null() {
        // A transport failure never produced a response, so there is no id to
        // record. It must read as unknown rather than as an empty-string key,
        // which would collide with every other id-less row during merging.
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "claude", "ClaudeOne").await;

        RoutePoolRepository::insert_request_event(
            &pool,
            &account_id,
            "route_proxy",
            r#"{"platform":"claude","success":false}"#,
            &RouteUsageBreakdown::default(),
            None,
        )
        .await
        .expect("insert");

        let stats = RoutePoolRepository::stats(&pool, "claude", None, 1, 20)
            .await
            .expect("stats");
        assert_eq!(stats.requests[0].upstream_response_id, None);
    }
```

- [ ] **Step 3: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_pool_repository::tests::request_event
```

期望：编译失败——`insert_request_event` 只接受 5 个参数，且 `RoutePoolUsageLog` 没有 `upstream_response_id` 字段。

- [ ] **Step 4: 实现**

在 `src-tauri/src/models/route_pool.rs` 的 `RoutePoolUsageLog` 末尾（`price_source` 之后）加字段：

```rust
    /// Upstream response id, when one could be read from the response body.
    /// The join key against a CLI transcript entry for the same request.
    pub upstream_response_id: Option<String>,
```

在 `route_pool_repository.rs` 改 `insert_request_event`：签名末尾加 `upstream_response_id: Option<&str>,`，SQL 的列清单加 `upstream_response_id`、`VALUES` 加一个 `?`，并在 `.bind(&usage.price_source)` 之后、`.bind(&now)` 之前插入 `.bind(upstream_response_id)`。

三处 `SELECT` 语句（`:422-432` 的 `recent_logs`、`:469-479` 的 `requests`）的列清单都要加 `ue.upstream_response_id`，并在 `map_usage_log` 闭包（`:497` 起）加 `upstream_response_id: row.get("upstream_response_id"),`。

在 `route_proxy_service.rs:5086` 的 `insert_route_credential_request_event` 签名加参数并透传：

```rust
async fn insert_route_credential_request_event(
    pool: &SqlitePool,
    route_credential_id: &str,
    metadata_json: &str,
    usage: &RouteUsageBreakdown,
    upstream_response_id: Option<&str>,
) -> Result<(), AppError> {
    RoutePoolRepository::insert_request_event(
        pool,
        route_credential_id,
        "route_proxy",
        metadata_json,
        usage,
        upstream_response_id,
    )
    .await
}
```

然后按编译器报错更新 8 处调用点。判断规则：**有响应字节的传抽取结果，没有的传 `None`**。

- 有响应体的两处（成功/重试路径）：`:1299` 附近的 buffered 路径已有 `response_bytes`，`:2083` 附近的流式完成路径已有 `preview`。这两处传
  `crate::services::upstream_response_id::extract_upstream_response_id(response_bytes.as_ref())`
  与 `...(&preview)`。
- 其余 6 处（`:703`、`:782`、`:851`、`:1093`、`:1200`、`:1960`）都是请求构造失败、刷新失败、流预热失败等没有响应体的场景（它们给 `route_proxy_request_metadata` 的 `response_body` 参数本来就是 `None`），传 `None`。

`route_model_test_service.rs:1372` 处已有 `response_body`（`String` 类型）：

```rust
    RoutePoolRepository::insert_request_event(
        pool,
        &credential.id,
        ROUTE_MODEL_TEST_SOURCE,
        &metadata,
        &usage,
        crate::services::upstream_response_id::extract_upstream_response_id(
            response_body.as_bytes(),
        )
        .as_deref(),
    )
    .await?;
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_pool
```

期望：新增 2 个测试通过，`route_pool_repository` 与 `route_pool_service` 的既有测试全绿。

- [ ] **Step 6: 跑全量 Rust 测试**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test
```

期望：全绿。`route_proxy_service` 与 `route_model_test_service` 的测试会构造 `RoutePoolUsageLog`，新字段可能需要补 `upstream_response_id: None`——按报错补。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/migrations/202609020003_usage_upstream_response_id.sql \
        src-tauri/src/models/route_pool.rs \
        src-tauri/src/database/repositories/route_pool_repository.rs \
        src-tauri/src/services/route_proxy_service.rs \
        src-tauri/src/services/route_model_test_service.rs
git commit -m "feat: 代理请求写入时记录上游响应 id"
```

---

### Task 4: 会话侧逐条记录与 Codex 逐轮拆分

这是全计划风险最高的一步。改的是有踩坑史的计数规则。

**Files:**
- Modify: `src-tauri/src/services/session_usage_service.rs`（`UsageEntry` 转公开、`parse_codex_file` 重写、新增 `collect_session_entries`）

**Interfaces:**
- Produces:
  - `pub struct SessionUsageEntry { pub provider: &'static str, pub model: String, pub response_id: Option<String>, pub timestamp_ms: Option<i64>, pub usage: TokenUsage }`
  - `pub fn collect_session_entries(window: TimeWindow) -> (Vec<SessionUsageEntry>, i64, bool)` —— 返回（去重后的逐条记录、扫描文件数、是否截断）

`SessionUsageEntry` 上**没有** `dedup_key`：跨文件去重在 `collect_session_entries` 内部就完成了，暴露出去只会让调用方以为还需要再去一次。内部的 `UsageEntry` 仍然保留 `dedup_key`。

**背景**（务必读完再动手）：

现有 `parse_codex_file`（`:504-560`）**刻意**只取每个 rollout 的最后一条 `total_token_usage`，因为该字段是**全会话累计值**。文件头注释（`:16-19`）记着：早期版本对它求和，一个真实文件多算了 350 倍（28.1B vs 实际 80.5M）。

要让 codex 请求进入合并列表，必须拆成逐轮。已在真实语料上验证过两种拆法：

- **用 `last_token_usage` 字段**（新版记录里有）：抽样 58 个可比文件，只有 12 个的 `Σ(last)` 等于最终累计值，46 个偏高。与 cc-switch #2571（fork 会话重复统计父会话历史 token）同源。**不要用这个。**
- **对相邻 `total_token_usage` 做差**：抽样 80 个文件，76 个的增量之和与最终累计值**完全相等**，1 个不等（fork 场景），3 个无可比数据。**用这个。**

做差规则：

1. 首个事件按原值计入。
2. 与上一次**完全相同**的累计值跳过（同一值会连发 2-3 次，实测一个文件里 6 次 token_count 有 3 对重复）。
3. 任一字段出现负差 → 判定会话重置/fork，该事件按原值重新起算。

Codex 的每轮去重键取自紧邻其前的 `response_item`：`rs_<uuid>` / `msg_<uuid>` / `fc_<uuid>` 都带上游响应 uuid。**排除 `fco_` 前缀**（客户端侧的 function_call_output，不是响应 id），并且只认 `role == "assistant"` 的 message——`developer` / `user` 角色的 message id 是客户端生成的会话内部 id（实测形如 `msg_01a0601e-...`，与响应 uuid 不同源）。最稳的是取 `reasoning`（`rs_`）与 `function_call`（`fc_`）的 id，它们只在助手响应里出现。

Claude 侧不动：已有的 `message.id` 既是 dedup_key 也是 response_id（同一个值），现有跨文件去重逻辑保留。

- [ ] **Step 1: 写 Codex 逐轮拆分的失败测试**

在 `session_usage_service.rs` 的 `#[cfg(test)] mod tests` 内新增。注意 `aggregate_codex` 等 helper 已存在（`:650`）：

```rust
    /// Build a Codex `token_count` event with the given cumulative totals.
    fn codex_token_count(ts: &str, input: i64, cached: i64, output: i64) -> String {
        format!(
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":{cached},"output_tokens":{output}}}}}}}}}"#
        )
    }

    #[test]
    fn codex_cumulative_totals_are_split_into_per_turn_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 100, 0, 10),
                &codex_token_count("2026-08-19T03:43:00.000Z", 300, 0, 30),
                &codex_token_count("2026-08-19T03:44:00.000Z", 1000, 200, 50),
            ],
        );

        let stats = aggregate_codex(&path);

        // Three turns, not one file-level row and not a triple-counted sum.
        assert_eq!(stats.totals.request_count, 3);
        // The per-turn deltas must add back up to the final cumulative value:
        // 1000 input of which 200 cached -> 800 uncached, 200 cache reads, 50 out.
        assert_eq!(stats.totals.input_tokens, 800);
        assert_eq!(stats.totals.cache_read_tokens, 200);
        assert_eq!(stats.totals.output_tokens, 50);
    }

    #[test]
    fn codex_repeated_identical_totals_are_not_counted_twice() {
        // A real rollout emits the same cumulative value 2-3 times in a row.
        // Each repeat would otherwise become a zero-token request, inflating the
        // request count without changing the tokens.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 100, 0, 10),
                &codex_token_count("2026-08-19T03:42:01.000Z", 100, 0, 10),
                &codex_token_count("2026-08-19T03:42:02.000Z", 100, 0, 10),
                &codex_token_count("2026-08-19T03:43:00.000Z", 300, 0, 30),
            ],
        );

        let stats = aggregate_codex(&path);

        assert_eq!(stats.totals.request_count, 2);
        assert_eq!(stats.totals.input_tokens, 300);
        assert_eq!(stats.totals.output_tokens, 30);
    }

    #[test]
    fn codex_negative_delta_restarts_the_running_total() {
        // A fork or resume resets the cumulative counter mid-file. Diffing
        // across that boundary would yield negative tokens; the event is
        // instead treated as a fresh start.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 1000, 0, 100),
                &codex_token_count("2026-08-19T03:43:00.000Z", 50, 0, 5),
                &codex_token_count("2026-08-19T03:44:00.000Z", 120, 0, 12),
            ],
        );

        let stats = aggregate_codex(&path);

        assert_eq!(stats.totals.request_count, 3);
        // 1000 (first) + 50 (restart) + 70 (delta) = 1120.
        assert_eq!(stats.totals.input_tokens, 1120);
        assert_eq!(stats.totals.output_tokens, 117);
    }

    #[test]
    fn codex_per_turn_deltas_sum_back_to_the_final_cumulative_total() {
        // The invariant that guards the 350x lesson recorded at the top of this
        // file: whatever the split produces must add back up to the last
        // cumulative value the session reported. If a future change reverts to
        // summing the cumulative values, this fails immediately.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 17063, 0, 500),
                &codex_token_count("2026-08-19T03:42:01.000Z", 17063, 0, 500),
                &codex_token_count("2026-08-19T03:43:00.000Z", 39956, 9600, 1200),
                &codex_token_count("2026-08-19T03:44:00.000Z", 76853, 30000, 2400),
            ],
        );

        let parsed = parse_codex_file(&path);
        let summed_input: i64 = parsed.entries.iter().map(|e| e.usage.input_tokens).sum();
        let summed_cache_read: i64 =
            parsed.entries.iter().map(|e| e.usage.cache_read_tokens).sum();
        let summed_output: i64 = parsed.entries.iter().map(|e| e.usage.output_tokens).sum();

        // Final cumulative: 76853 input of which 30000 cached -> 46853 uncached.
        assert_eq!(summed_input, 46_853);
        assert_eq!(summed_cache_read, 30_000);
        assert_eq!(summed_output, 2_400);
    }

    #[test]
    fn codex_turn_response_id_comes_from_assistant_items_only() {
        // `rs_` (reasoning) and `fc_` (function_call) ids embed the upstream
        // Responses uuid and only appear in assistant output. `fco_` is the
        // client's own function_call_output and `msg_` on a user/developer turn
        // is a client-side conversation id — neither can join to a proxy row.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                r#"{"timestamp":"2026-08-19T03:41:51.000Z","type":"response_item","payload":{"type":"message","role":"user","id":"msg_01a0601e-a66f-7c42-aabe-6488c2cf7b61"}}"#,
                r#"{"timestamp":"2026-08-19T03:41:52.000Z","type":"response_item","payload":{"type":"reasoning","id":"rs_5d76e101-2615-4e87-8455-72061b36392c"}}"#,
                r#"{"timestamp":"2026-08-19T03:41:53.000Z","type":"response_item","payload":{"type":"function_call_output","id":"fco_01a0601e-fbbc-7e40-be4a-7d80109de16b"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 100, 0, 10),
            ],
        );

        let parsed = parse_codex_file(&path);

        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(
            parsed.entries[0].response_id.as_deref(),
            Some("5d76e101-2615-4e87-8455-72061b36392c")
        );
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib session_usage_service::tests::codex
```

期望：`codex_cumulative_totals_are_split_into_per_turn_entries` 断言 `request_count == 3` 但实得 1（现在每文件只产出一条）；`codex_turn_response_id_...` 编译失败（`UsageEntry` 没有 `response_id` 字段）。

现有的 `codex_counts_only_the_final_cumulative_total`（`:741`）会失败——它断言的正是被替换掉的旧行为。**Step 4 会把它改写**，先不动。

- [ ] **Step 3: 实现 Codex 逐轮拆分**

给 `UsageEntry`（`:124-132`）加字段：

```rust
    /// Upstream response id, the join key against a proxy usage row.
    response_id: Option<String>,
```

Claude 侧（`parse_claude_file`，`:471-484`）把 `message.id` 同时填进两个字段——它既是跨文件去重键，也是上游响应 id：

```rust
        let message_id = message
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);

        entries.push(UsageEntry {
            provider: "claude",
            model: model.to_string(),
            dedup_key: message_id.clone(),
            response_id: message_id,
            timestamp_ms: entry_timestamp_ms(&entry),
            usage: claude_token_usage(usage),
        });
```

重写 `parse_codex_file`：

```rust
/// Parse one Codex CLI rollout into its per-turn billable entries.
///
/// `total_token_usage` accumulates over the session, so each turn is the
/// difference from the previous event rather than the value itself. Summing the
/// raw values overstated one real file by 350x (see the module header).
///
/// `last_token_usage` looks like it would serve directly, but on a real corpus
/// only 12 of 58 comparable files had `Σ(last)` equal the final cumulative
/// total — forked sessions re-report the parent's history. Diffing matched on
/// 76 of 77.
fn parse_codex_file(path: &Path) -> ParsedFile {
    let Some(lines) = read_lines(path) else {
        return ParsedFile::default();
    };

    let mut model: Option<String> = None;
    let mut previous: Option<CodexCumulative> = None;
    let mut pending_response_id: Option<String> = None;
    let mut entries = Vec::new();

    for line in lines {
        // Cheap pre-filter: only `turn_context` (the model), `response_item`
        // (the response id), and `token_count` events matter.
        if !line.contains("token_count")
            && !line.contains("\"model\"")
            && !line.contains("response_item")
        {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let payload = entry.get("payload").unwrap_or(&Value::Null);

        if let Some(found) = payload
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            model = Some(found.to_string());
        }

        if let Some(id) = codex_assistant_response_id(payload) {
            pending_response_id = Some(id);
        }

        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let Some(total) = payload.pointer("/info/total_token_usage") else {
            continue;
        };
        let current = CodexCumulative::from_value(total);

        let delta = match previous {
            // The same cumulative value is emitted 2-3 times in a row; only the
            // first occurrence is a turn.
            Some(previous) if previous == current => continue,
            Some(previous) => current.delta_from(previous),
            None => Some(current.usage()),
        };
        // A negative delta means the session counter reset (fork or resume), so
        // the event starts a fresh running total instead of being diffed.
        let usage = delta.unwrap_or_else(|| current.usage());

        entries.push(UsageEntry {
            provider: "codex",
            // A rollout without a recorded model still represents real spend;
            // attribute it to a placeholder so it appears as unpriced rather
            // than vanishing from the totals.
            model: model.clone().unwrap_or_else(|| "unknown".to_string()),
            // Codex has no cross-file message id; the response id below is the
            // merge key, not a dedup key.
            dedup_key: None,
            response_id: pending_response_id.take(),
            timestamp_ms: entry_timestamp_ms(&entry),
            usage,
        });
        previous = Some(current);
    }

    ParsedFile { entries }
}

/// Cumulative token counts as Codex reports them, before cache adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodexCumulative {
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
}

impl CodexCumulative {
    fn from_value(total: &Value) -> Self {
        Self {
            input_tokens: json_i64(total.get("input_tokens")),
            cached_input_tokens: json_i64(total.get("cached_input_tokens")),
            cache_write_input_tokens: json_i64(total.get("cache_write_input_tokens")),
            // `reasoning_output_tokens` is already part of `output_tokens`;
            // adding it would double-count reasoning.
            output_tokens: json_i64(total.get("output_tokens")),
        }
    }

    /// This event's own usage, treating the cumulative value as the whole turn.
    fn usage(self) -> TokenUsage {
        // Codex reports `input_tokens` inclusive of `cached_input_tokens`, so
        // the cached portion is subtracted to avoid billing it at the full
        // input rate.
        TokenUsage {
            input_tokens: (self.input_tokens - self.cached_input_tokens).max(0),
            output_tokens: self.output_tokens,
            cache_write_tokens: self.cache_write_input_tokens,
            cache_read_tokens: self.cached_input_tokens,
        }
    }

    /// Usage attributable to this turn alone, or `None` when any field went
    /// backwards (the session counter reset).
    fn delta_from(self, previous: Self) -> Option<TokenUsage> {
        let input = self.input_tokens - previous.input_tokens;
        let cached = self.cached_input_tokens - previous.cached_input_tokens;
        let cache_write = self.cache_write_input_tokens - previous.cache_write_input_tokens;
        let output = self.output_tokens - previous.output_tokens;
        if input < 0 || cached < 0 || cache_write < 0 || output < 0 {
            return None;
        }
        Some(TokenUsage {
            input_tokens: (input - cached).max(0),
            output_tokens: output,
            cache_write_tokens: cache_write,
            cache_read_tokens: cached,
        })
    }
}

/// The upstream Responses uuid embedded in an assistant `response_item` id.
///
/// `rs_` (reasoning) and `fc_` (function_call) only ever appear in assistant
/// output. `fco_` is the client's own function_call_output, and a `msg_` on a
/// user or developer turn is a client-side conversation id — neither joins to a
/// proxy row, so both are rejected.
fn codex_assistant_response_id(payload: &Value) -> Option<String> {
    let item_type = payload.get("type").and_then(Value::as_str)?;
    let id = payload.get("id").and_then(Value::as_str)?;
    let uuid = match item_type {
        "reasoning" => id.strip_prefix("rs_"),
        "function_call" => id.strip_prefix("fc_").map(|rest| {
            // Function-call ids carry a trailing index: fc_<uuid>_0.
            rest.rsplit_once('_').map_or(rest, |(head, _)| head)
        }),
        "message" if payload.get("role").and_then(Value::as_str) == Some("assistant") => {
            id.strip_prefix("msg_")
        }
        _ => None,
    }?;
    (!uuid.trim().is_empty()).then(|| uuid.to_string())
}
```

- [ ] **Step 4: 改写被替换的旧行为测试**

`codex_counts_only_the_final_cumulative_total`（`:741-763`）断言的是「每文件一条」，与新行为冲突。改名并改写为新语义，保留它对「不许求和」的守护意图：

```rust
    #[test]
    fn codex_turns_never_sum_the_cumulative_totals() {
        // The original form of this test pinned "one row per file". Turns are
        // now split out per event, but the property it guarded still holds: the
        // tokens must equal the final cumulative value, never the sum of every
        // cumulative snapshot (which overstated one real file by 350x).
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_jsonl(
            dir.path(),
            "rollout.jsonl",
            &[
                r#"{"timestamp":"2026-08-19T03:41:50.476Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}"#,
                &codex_token_count("2026-08-19T03:42:00.000Z", 100, 0, 10),
                &codex_token_count("2026-08-19T03:43:00.000Z", 300, 0, 30),
                &codex_token_count("2026-08-19T03:44:00.000Z", 1000, 200, 50),
            ],
        );

        let stats = aggregate_codex(&path);

        // Summing the snapshots would give 1400 input; the correct answer is
        // the final cumulative value, 1000, minus the 200 cached portion.
        assert_eq!(stats.totals.input_tokens, 800);
        assert_eq!(stats.totals.cache_read_tokens, 200);
        assert_eq!(stats.totals.output_tokens, 50);
        assert_eq!(stats.by_model[0].model, "gpt-5.6-sol");
    }
```

`codex_rollout_without_model_is_reported_as_unpriced`（`:766-783`）只有一个 token_count 事件，新逻辑下仍产出一条，断言不变，无需改动。

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib session_usage_service
```

期望：全绿。

- [ ] **Step 6: 用真实语料验证不变量**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib real_corpus -- --ignored --nocapture
```

这个 `#[ignore]` 测试（`:944-1009`）读真实的 `~/.codex` 与 `~/.claude`。对比改动前后的输出：**requests 会显著变多**（codex 从每文件 1 条变成每轮 1 条），但 **codex 的 token 总量应基本不变**（做差之和等于最终累计值）。若 token 总量暴涨，说明退回了求和，立刻停下排查。

- [ ] **Step 7: 暴露逐条记录**

`UsageEntry` 是私有的（`:124`），合并服务需要它。新增一个公开的等价类型与入口，而不是直接把内部类型公开——内部类型带 `&'static str` 与解析细节，不适合做服务边界：

```rust
/// One billable request from a transcript, after time filtering and dedup.
///
/// The public counterpart of the internal parse entry, for callers that merge
/// transcript records with proxy usage rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionUsageEntry {
    /// `claude` or `codex`.
    pub provider: &'static str,
    pub model: String,
    /// Upstream response id: the join key against a proxy usage row.
    pub response_id: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub usage: TokenUsage,
}

/// Scan local transcripts and return the deduplicated per-request entries
/// within `window`, plus the scanned file count and whether the cap was hit.
///
/// Shares the parse cache with [`scan_session_usage`], so a caller that needs
/// both rows and rollups pays for the file reads once.
///
/// Blocking file IO — call from `spawn_blocking`.
pub fn collect_session_entries(window: TimeWindow) -> (Vec<SessionUsageEntry>, i64, bool) {
    let mut entries = Vec::new();
    let mut scanned = 0_i64;
    let mut truncated = false;
    let mut seen_dedup_keys = HashSet::new();

    let roots = claude_roots()
        .into_iter()
        .map(|root| (root, Provider::Claude))
        .chain(codex_roots().into_iter().map(|root| (root, Provider::Codex)));

    for (root, provider) in roots {
        if !root.exists() {
            continue;
        }
        let files = collect_files(&root, &mut truncated);
        scanned += files.len() as i64;
        for path in files {
            let parsed = parsed_file(&path, provider);
            for entry in &parsed.entries {
                if !window.contains(entry.timestamp_ms) {
                    continue;
                }
                if let Some(key) = &entry.dedup_key {
                    if !seen_dedup_keys.insert(key.clone()) {
                        continue;
                    }
                }
                entries.push(SessionUsageEntry {
                    provider: entry.provider,
                    model: entry.model.clone(),
                    response_id: entry.response_id.clone(),
                    timestamp_ms: entry.timestamp_ms,
                    usage: entry.usage,
                });
            }
        }
    }

    (entries, scanned, truncated)
}
```

加测试：

```rust
    #[test]
    fn collected_entries_carry_the_response_id_and_respect_dedup() {
        // Mirrors what collect_session_entries does, without touching the real
        // home directory: the same message id in two files yields one entry,
        // and the response id survives for merging.
        let dir = tempfile::tempdir().expect("tempdir");
        let first = write_jsonl(
            dir.path(),
            "a.jsonl",
            &[&claude_line("msg_shared", "claude-opus-5", 500, 0)],
        );
        let second = write_jsonl(
            dir.path(),
            "b.jsonl",
            &[&claude_line("msg_shared", "claude-opus-5", 500, 0)],
        );

        let mut seen = HashSet::new();
        let mut collected = Vec::new();
        for path in [&first, &second] {
            for entry in parse_claude_file(path).entries {
                if let Some(key) = &entry.dedup_key {
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                }
                collected.push(entry);
            }
        }

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].response_id.as_deref(), Some("msg_shared"));
    }
```

- [ ] **Step 8: 运行全量 Rust 测试**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test
```

期望：全绿。

- [ ] **Step 9: 提交**

```bash
git add src-tauri/src/services/session_usage_service.rs
git commit -m "$(cat <<'EOF'
feat: Codex 会话用量按轮拆分并暴露逐条记录

total_token_usage 是全会话累计值，改为对相邻事件做差得到每轮增量。
不用 last_token_usage：真实语料 58 个可比文件里只有 12 个的求和等于
最终累计值，fork 会话会重复上报父会话历史。做差法 77 个里 76 个精确相等。

新增不变量测试守住模块头记录的 350 倍教训。
EOF
)"
```

---

### Task 5a: 读取时间窗内的全部代理请求行

合并服务需要跨平台、不分页的全量 request 行。现有 `RoutePoolRepository::stats` 是按平台且带分页的，不能复用。

**Files:**
- Modify: `src-tauri/src/database/repositories/route_pool_repository.rs`（新增方法）

**Interfaces:**
- Produces: `RoutePoolRepository::list_request_events(pool: &SqlitePool, since: Option<&str>) -> Result<Vec<ProxyRequestRow>, AppError>`
- Produces: `pub struct ProxyRequestRow`（放在 `src-tauri/src/models/route_pool.rs`）

**两处与现有查询不同的地方，都是「总花费」语义的直接后果**：

1. **不按平台过滤** —— 顶部数字跨 provider。行自带 `platform` 供分组。
2. **不排除已归档账号** —— 现有查询一律带 `a.archived_at IS NULL`。归档账号过去发生的请求仍是真实花费，排除会让总数偏低。归档只影响账号列表的可见性，不该改写历史账单。

- [ ] **Step 1: 写失败测试**

在 `route_pool_repository.rs` 的测试模块内新增：

```rust
    #[tokio::test]
    async fn list_request_events_spans_platforms_and_includes_archived_accounts() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let claude_id = account(&pool, "claude", "ClaudeOne").await;
        let codex_id = account(&pool, "codex", "CodexOne").await;

        for (account_id, response_id) in [(&claude_id, "msg_a"), (&codex_id, "resp_b")] {
            RoutePoolRepository::insert_request_event(
                &pool,
                account_id,
                "route_proxy",
                r#"{"success":true}"#,
                &RouteUsageBreakdown::default(),
                Some(response_id),
            )
            .await
            .expect("insert");
        }

        // Archiving an account hides it from the account list; it must not erase
        // the spend it already incurred.
        sqlx::query("UPDATE route_credentials SET archived_at = ? WHERE id = ?")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(&claude_id)
            .execute(&pool)
            .await
            .expect("archive");

        let rows = RoutePoolRepository::list_request_events(&pool, None)
            .await
            .expect("rows");

        assert_eq!(rows.len(), 2, "both platforms, archived included");
        let platforms: HashSet<&str> = rows.iter().map(|row| row.platform.as_str()).collect();
        assert!(platforms.contains("claude") && platforms.contains("codex"));
    }

    #[tokio::test]
    async fn list_request_events_filters_by_since_and_excludes_non_request_metrics() {
        let pool = create_memory_pool().await.expect("pool");
        run_migrations(&pool).await.expect("migrations");
        let account_id = account(&pool, "codex", "CodexOne").await;

        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            "{}",
            "2026-08-01T00:00:00Z",
        )
        .await;
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "request",
            1,
            "count",
            "{}",
            "2026-08-20T00:00:00Z",
        )
        .await;
        // A legacy token row must not become a phantom request.
        usage_event_at(
            &pool,
            &account_id,
            "route_proxy",
            "token",
            4096,
            "token",
            "{}",
            "2026-08-20T00:00:00Z",
        )
        .await;

        let rows = RoutePoolRepository::list_request_events(&pool, Some("2026-08-10T00:00:00Z"))
            .await
            .expect("rows");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].created_at, "2026-08-20T00:00:00Z");
    }
```

测试模块顶部若无 `HashSet` 导入，加 `use std::collections::HashSet;`。

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_pool_repository::tests::list_request_events
```

期望：编译失败，`no function or associated item named list_request_events`。

- [ ] **Step 3: 实现**

在 `src-tauri/src/models/route_pool.rs` 新增：

```rust
/// One proxied request as stored in `usage_events`, for the usage overview.
///
/// Unlike [`RoutePoolUsageLog`] this carries the owning credential's platform
/// and is never filtered by platform or archive state: the overview reports
/// total spend, and an archived account's past requests are still spend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyRequestRow {
    pub id: String,
    pub platform: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub source_label: String,
    pub metadata_json: String,
    pub created_at: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_tokens: Option<i64>,
    pub price_usd_micros: Option<i64>,
    pub price_cny_micros: Option<i64>,
    pub price_currency: Option<String>,
    pub price_source: Option<String>,
    pub upstream_response_id: Option<String>,
}
```

在 `route_pool_repository.rs` 的 `impl RoutePoolRepository` 内新增：

```rust
    /// Every proxied request in the window, across all platforms.
    ///
    /// Deliberately unfiltered by platform and by `archived_at`: the usage
    /// overview reports total spend, so a request stays counted after its
    /// account is archived. Archiving governs the account list, not history.
    pub async fn list_request_events(
        pool: &SqlitePool,
        since: Option<&str>,
    ) -> Result<Vec<ProxyRequestRow>, AppError> {
        let since_clause = if since.is_some() {
            " AND ue.created_at >= ?"
        } else {
            ""
        };
        let sql = format!(
            "SELECT ue.id, a.platform AS platform, ue.route_credential_id,
                    a.display_name AS account_name, ue.source_label,
                    ue.metadata_json, ue.created_at,
                    ue.input_tokens, ue.output_tokens, ue.cache_tokens,
                    ue.price_usd_micros, ue.price_cny_micros, ue.price_currency,
                    ue.price_source, ue.upstream_response_id
             FROM usage_events ue
             INNER JOIN route_credentials a ON a.id = ue.route_credential_id
             WHERE ue.metric_type = 'request'{since_clause}
             ORDER BY ue.created_at DESC, ue.id DESC"
        );
        let mut query = sqlx::query(&sql);
        if let Some(since) = since {
            query = query.bind(since);
        }
        let rows = query
            .fetch_all(pool)
            .await
            .map_err(|err| AppError::Database {
                code: "database.usage_overview_requests",
                message: "Could not load proxied requests".to_string(),
                details: Some(err.to_string()),
                recoverable: true,
            })?;

        Ok(rows
            .into_iter()
            .map(|row| ProxyRequestRow {
                id: row.get("id"),
                platform: row.get("platform"),
                account_id: row.get("route_credential_id"),
                account_name: row.get("account_name"),
                source_label: row.get("source_label"),
                metadata_json: row.get("metadata_json"),
                created_at: row.get("created_at"),
                input_tokens: row.get("input_tokens"),
                output_tokens: row.get("output_tokens"),
                cache_tokens: row.get("cache_tokens"),
                price_usd_micros: row.get("price_usd_micros"),
                price_cny_micros: row.get("price_cny_micros"),
                price_currency: row.get("price_currency"),
                price_source: row.get("price_source"),
                upstream_response_id: row.get("upstream_response_id"),
            })
            .collect())
    }
```

在文件顶部的 `use crate::models::route_pool::{...}` 里加 `ProxyRequestRow`。

- [ ] **Step 4: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib route_pool_repository
```

期望：全绿。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/models/route_pool.rs src-tauri/src/database/repositories/route_pool_repository.rs
git commit -m "feat: 新增跨平台读取代理请求行的查询"
```

---

### Task 5b: 合并服务

计划的核心。把会话记录与代理行合并去重、汇总、四维分组、分页。

**Files:**
- Create: `src-tauri/src/services/usage_overview_service.rs`
- Modify: `src-tauri/src/services/mod.rs`（加 `pub mod usage_overview_service;`，字母序在 `upstream_response_id` 之后）

**Interfaces:**
- Consumes:
  - `session_usage_service::collect_session_entries(window) -> (Vec<SessionUsageEntry>, i64, bool)`（Task 4）
  - `RoutePoolRepository::list_request_events(pool, since) -> Vec<ProxyRequestRow>`（Task 5a）
  - `upstream_response_id::extract_upstream_response_id(body) -> Option<String>`（Task 1）
  - `model_pricing::{estimate_cost_micros, cny_micros_to_usd_micros, TokenUsage}`
- Produces:
  - `pub struct UsageOverviewRow`、`UsageOverviewTotals`、`UsageOverviewGroupRow`、`UsageOverviewGroups`、`UsageOverviewIntegrity`、`UsageOverview`
  - `pub async fn build_usage_overview(pool: &SqlitePool, since: Option<&str>, page: i64, page_size: i64, window: TimeWindow) -> Result<UsageOverview, AppError>`（Task 5c 实现）

`since` 与 `window` 都传是有意的，不是冗余：两者源自同一个输入但用途不同——`since` 是 RFC 3339 字符串，直接进 SQL 的 `created_at >= ?` 比较；`window` 是 epoch 毫秒，给会话记录的时间过滤（`TimeWindow::contains`）。让调用方（`core::usage_overview`）一次解析、两种形态各用一处，比在服务里再解析一遍字符串更稳。

**合并规则**（spec「合并行的字段取舍」）：

| 字段 | 匹配行取自 | 理由 |
| --- | --- | --- |
| token | 会话侧 | 缓存拆成写入/读取，定价差 12.5 倍；代理侧流截断会漏最后一个 delta |
| 费用 | 代理侧上游价格优先（`price_source == "upstream"`），否则按会话 token 本地估算 | 上游价格是真实计费 |
| 账号/状态码/路径 | 代理侧 | 会话记录没有 |

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/services/usage_overview_service.rs`，先只写类型骨架 + 测试：

```rust
//! Merge local CLI transcript usage with proxied request rows into one list.
//!
//! The two sources overlap: a CLI request routed through this app's proxy is
//! recorded on both sides. The upstream response id joins them — on a real
//! corpus 2905 of 2933 proxy rows (99.0%) matched a transcript entry. Merging
//! on that key is what lets a single set of totals mean "my total spend"
//! instead of double counting the overlap.

use crate::database::repositories::route_pool_repository::RoutePoolRepository;
use crate::error::AppError;
use crate::models::route_pool::ProxyRequestRow;
use crate::services::model_pricing::{self, TokenUsage};
use crate::services::session_usage_service::{self, SessionUsageEntry, TimeWindow};
use crate::services::upstream_response_id::extract_upstream_response_id;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Where a merged row's data came from. Doubles as the "source" grouping key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageRowSource {
    /// Present on both sides: a CLI request that went through this proxy.
    Matched,
    /// Transcript only: the CLI reached the upstream directly.
    SessionOnly,
    /// Proxy only: the caller is not one of the scanned CLIs (model test, or
    /// another tool pointed at this proxy).
    ProxyOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewRow {
    /// Stable id for React keys: the proxy row id, else the response id, else a
    /// synthesized `session:<index>`.
    pub id: String,
    pub source: UsageRowSource,
    /// RFC 3339. Proxy `created_at` for matched and proxy-only rows, the
    /// transcript timestamp for session-only rows.
    pub occurred_at: Option<String>,
    pub provider: String,
    pub model: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub source_label: Option<String>,
    pub path: Option<String>,
    /// HTTP status, only ever present on a row with a proxy side.
    pub status: Option<String>,
    pub success: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_micros: i64,
    /// `upstream` when a real upstream price was used, `estimated` when the
    /// local price table was, `null` when the model has no known rate.
    pub price_source: Option<String>,
    pub upstream_response_id: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewTotals {
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_micros: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewGroupRow {
    /// Display label: the model id, platform id, account name, or source name.
    pub key: String,
    #[serde(flatten)]
    pub totals: UsageOverviewTotals,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewGroups {
    pub by_model: Vec<UsageOverviewGroupRow>,
    pub by_platform: Vec<UsageOverviewGroupRow>,
    pub by_account: Vec<UsageOverviewGroupRow>,
    pub by_source: Vec<UsageOverviewGroupRow>,
}

/// Facts the UI needs to state how complete the totals are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverviewIntegrity {
    pub scanned_file_count: i64,
    /// True when the transcript file cap was hit, so the totals are a floor.
    pub truncated: bool,
    /// Requests whose model has no rate, contributing no cost.
    pub unpriced_request_count: i64,
    /// Requests priced from the local table rather than an upstream price.
    pub estimated_price_request_count: i64,
    /// Proxy rows with no response id, which therefore could not be merged and
    /// may double count against a transcript entry for the same request.
    pub unmatchable_proxy_row_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageOverview {
    pub totals: UsageOverviewTotals,
    pub rows: Vec<UsageOverviewRow>,
    pub groups: UsageOverviewGroups,
    pub row_count: i64,
    pub page: i64,
    pub page_size: i64,
    pub integrity: UsageOverviewIntegrity,
}

pub fn merge_entries(
    _session_entries: Vec<SessionUsageEntry>,
    _proxy_rows: Vec<ProxyRequestRow>,
) -> Vec<UsageOverviewRow> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_entry(response_id: Option<&str>, model: &str, input: i64) -> SessionUsageEntry {
        SessionUsageEntry {
            provider: "claude",
            model: model.to_string(),
            response_id: response_id.map(str::to_string),
            timestamp_ms: Some(1_787_000_000_000),
            usage: TokenUsage {
                input_tokens: input,
                output_tokens: 20,
                cache_write_tokens: 5,
                cache_read_tokens: 7,
            },
        }
    }

    fn proxy_row(response_id: Option<&str>) -> ProxyRequestRow {
        ProxyRequestRow {
            id: "proxy-1".to_string(),
            platform: "claude".to_string(),
            account_id: Some("cred-1".to_string()),
            account_name: Some("Team Account".to_string()),
            source_label: "route_proxy".to_string(),
            metadata_json: r#"{"path":"/v1/messages","status":200,"success":true,"upstream_model":"claude-opus-5"}"#.to_string(),
            created_at: "2026-08-19T14:04:50Z".to_string(),
            // Deliberately different from the session entry so the field
            // precedence is observable.
            input_tokens: Some(999),
            output_tokens: Some(999),
            cache_tokens: Some(999),
            price_usd_micros: Some(4_200),
            price_cny_micros: None,
            price_currency: Some("usd".to_string()),
            price_source: Some("upstream".to_string()),
            upstream_response_id: response_id.map(str::to_string),
        }
    }

    #[test]
    fn a_matched_pair_becomes_one_row() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows.len(), 1, "the overlap must not be counted twice");
        assert_eq!(rows[0].source, UsageRowSource::Matched);
    }

    #[test]
    fn a_matched_row_takes_tokens_from_the_transcript() {
        // The transcript splits cache into write and read, which price 12.5x
        // apart, and it does not lose the final delta of a truncated stream.
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].input_tokens, 120);
        assert_eq!(rows[0].output_tokens, 20);
        assert_eq!(rows[0].cache_write_tokens, 5);
        assert_eq!(rows[0].cache_read_tokens, 7);
    }

    #[test]
    fn a_matched_row_takes_account_and_status_from_the_proxy() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].account_name.as_deref(), Some("Team Account"));
        assert_eq!(rows[0].status.as_deref(), Some("200"));
        assert_eq!(rows[0].path.as_deref(), Some("/v1/messages"));
        assert!(rows[0].success);
    }

    #[test]
    fn an_upstream_price_wins_over_a_local_estimate() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![proxy_row(Some("msg_a"))],
        );

        assert_eq!(rows[0].cost_micros, 4_200);
        assert_eq!(rows[0].price_source.as_deref(), Some("upstream"));
    }

    #[test]
    fn a_cny_upstream_price_is_converted_to_usd() {
        let mut row = proxy_row(Some("msg_a"));
        row.price_usd_micros = None;
        row.price_cny_micros = Some(7_100_000);
        row.price_currency = Some("cny".to_string());

        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 120)],
            vec![row],
        );

        // 7.1 CNY at the fixed 7.1 rate is exactly 1 USD.
        assert_eq!(rows[0].cost_micros, 1_000_000);
    }

    #[test]
    fn a_matched_row_without_an_upstream_price_is_estimated_from_transcript_tokens() {
        let mut row = proxy_row(Some("msg_a"));
        row.price_usd_micros = None;
        row.price_cny_micros = None;
        row.price_currency = None;
        row.price_source = None;

        let rows = merge_entries(
            vec![session_entry(Some("msg_a"), "claude-opus-5", 1_000_000)],
            vec![row],
        );

        assert_eq!(rows[0].price_source.as_deref(), Some("estimated"));
        // Priced off the transcript's 1M input tokens, not the proxy's 999.
        assert!(
            rows[0].cost_micros > 1_000_000,
            "1M input tokens must cost more than $1, got {}",
            rows[0].cost_micros
        );
    }

    #[test]
    fn unmatched_rows_from_each_side_are_kept_and_labelled() {
        let rows = merge_entries(
            vec![session_entry(Some("msg_only_session"), "claude-opus-5", 10)],
            vec![proxy_row(Some("msg_only_proxy"))],
        );

        assert_eq!(rows.len(), 2);
        let sources: Vec<UsageRowSource> = rows.iter().map(|row| row.source).collect();
        assert!(sources.contains(&UsageRowSource::SessionOnly));
        assert!(sources.contains(&UsageRowSource::ProxyOnly));
    }

    #[test]
    fn rows_without_a_response_id_never_merge_with_each_other() {
        // Two id-less rows are not evidence of the same request. Treating a
        // missing key as a shared key would collapse unrelated requests and
        // undercount spend.
        let rows = merge_entries(
            vec![session_entry(None, "claude-opus-5", 10)],
            vec![proxy_row(None)],
        );

        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_proxy_row_falls_back_to_parsing_its_stored_body_for_an_id() {
        // Pre-migration rows have no upstream_response_id column value; the id
        // is still recoverable from the stored response preview.
        let mut row = proxy_row(None);
        row.metadata_json = r#"{"path":"/v1/messages","status":200,"success":true,"response_body":"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_legacy\"}}\n\n"}"#.to_string();

        let rows = merge_entries(
            vec![session_entry(Some("msg_legacy"), "claude-opus-5", 120)],
            vec![row],
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, UsageRowSource::Matched);
    }

    #[test]
    fn rows_are_ordered_newest_first() {
        let mut older = session_entry(Some("msg_old"), "claude-opus-5", 10);
        older.timestamp_ms = Some(1_786_000_000_000);
        let mut newer = session_entry(Some("msg_new"), "claude-opus-5", 10);
        newer.timestamp_ms = Some(1_788_000_000_000);

        let rows = merge_entries(vec![older, newer], Vec::new());

        assert_eq!(rows[0].upstream_response_id.as_deref(), Some("msg_new"));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib usage_overview_service
```

期望：10 个测试大部分 FAIL（`merge_entries` 返回空 vec，`rows.len()` 断言全挂）。

- [ ] **Step 3: 实现 merge_entries**

替换空实现：

```rust
/// Merge the two sides on the upstream response id.
///
/// A row with no id on either side stays unmerged: a missing key is not
/// evidence of a shared request, and treating it as one would collapse
/// unrelated requests into a single row.
pub fn merge_entries(
    session_entries: Vec<SessionUsageEntry>,
    proxy_rows: Vec<ProxyRequestRow>,
) -> Vec<UsageOverviewRow> {
    // Index the proxy side by response id, falling back to parsing the stored
    // body preview for rows written before the column existed.
    let mut proxy_by_id: HashMap<String, ProxyRequestRow> = HashMap::new();
    let mut unkeyed_proxy_rows = Vec::new();
    for row in proxy_rows {
        match resolve_proxy_response_id(&row) {
            Some(id) => {
                proxy_by_id.insert(id, row);
            }
            None => unkeyed_proxy_rows.push(row),
        }
    }

    let mut rows = Vec::new();
    for (index, entry) in session_entries.into_iter().enumerate() {
        let paired = entry
            .response_id
            .as_deref()
            .and_then(|id| proxy_by_id.remove(id));
        rows.push(match paired {
            Some(proxy) => merged_row(entry, proxy),
            None => session_only_row(entry, index),
        });
    }

    // Whatever the transcripts never claimed is proxy-only: a model test, or a
    // tool other than the two scanned CLIs pointed at this proxy.
    for row in proxy_by_id.into_values().chain(unkeyed_proxy_rows) {
        rows.push(proxy_only_row(row));
    }

    rows.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at));
    rows
}

fn resolve_proxy_response_id(row: &ProxyRequestRow) -> Option<String> {
    if let Some(id) = row
        .upstream_response_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Some(id.to_string());
    }
    // Pre-migration rows: the id may still be inside the stored preview.
    let metadata = serde_json::from_str::<serde_json::Value>(&row.metadata_json).ok()?;
    let body = metadata.get("response_body")?.as_str()?;
    extract_upstream_response_id(body.as_bytes())
}

struct ProxyFacts {
    path: Option<String>,
    status: Option<String>,
    success: bool,
    model: Option<String>,
}

fn proxy_facts(row: &ProxyRequestRow) -> ProxyFacts {
    let metadata = serde_json::from_str::<serde_json::Value>(&row.metadata_json).ok();
    let field = |key: &str| -> Option<String> {
        let value = metadata.as_ref()?.get(key)?;
        match value {
            serde_json::Value::String(text) if !text.trim().is_empty() => {
                Some(text.trim().to_string())
            }
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    };
    ProxyFacts {
        path: field("path"),
        status: field("status"),
        // Absent `success` means a legacy row that only recorded successes.
        success: metadata
            .as_ref()
            .and_then(|value| value.get("success"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        model: field("upstream_model").or_else(|| field("requested_model")),
    }
}

/// The upstream's own price in USD micros, when it reported one.
fn upstream_cost_micros(row: &ProxyRequestRow) -> Option<i64> {
    if row.price_source.as_deref() != Some("upstream") {
        return None;
    }
    match row.price_currency.as_deref() {
        Some("usd") => row.price_usd_micros,
        Some("cny") => row
            .price_cny_micros
            .map(model_pricing::cny_micros_to_usd_micros),
        _ => None,
    }
}

fn merged_row(entry: SessionUsageEntry, proxy: ProxyRequestRow) -> UsageOverviewRow {
    let facts = proxy_facts(&proxy);
    // An upstream price is real billing data; a local estimate is a guess.
    let (cost_micros, price_source) = match upstream_cost_micros(&proxy) {
        Some(cost) => (cost, Some("upstream".to_string())),
        None => estimated_cost(&entry.model, entry.usage),
    };
    UsageOverviewRow {
        id: proxy.id,
        source: UsageRowSource::Matched,
        occurred_at: Some(proxy.created_at),
        provider: entry.provider.to_string(),
        // The transcript records what the CLI itself used; the proxy metadata
        // covers the case of a gateway serving a different model.
        model: entry.model,
        account_id: proxy.account_id,
        account_name: proxy.account_name,
        source_label: Some(proxy.source_label),
        path: facts.path,
        status: facts.status,
        success: facts.success,
        input_tokens: entry.usage.input_tokens.max(0),
        output_tokens: entry.usage.output_tokens.max(0),
        cache_write_tokens: entry.usage.cache_write_tokens.max(0),
        cache_read_tokens: entry.usage.cache_read_tokens.max(0),
        cost_micros,
        price_source,
        upstream_response_id: entry.response_id,
        metadata_json: Some(proxy.metadata_json),
    }
}

fn session_only_row(entry: SessionUsageEntry, index: usize) -> UsageOverviewRow {
    let (cost_micros, price_source) = estimated_cost(&entry.model, entry.usage);
    UsageOverviewRow {
        id: entry
            .response_id
            .clone()
            .unwrap_or_else(|| format!("session:{index}")),
        source: UsageRowSource::SessionOnly,
        occurred_at: entry.timestamp_ms.and_then(rfc3339_from_millis),
        provider: entry.provider.to_string(),
        model: entry.model,
        account_id: None,
        account_name: None,
        source_label: None,
        path: None,
        // A transcript has no HTTP status; an entry exists only for a request
        // that returned usage, so it succeeded.
        status: None,
        success: true,
        input_tokens: entry.usage.input_tokens.max(0),
        output_tokens: entry.usage.output_tokens.max(0),
        cache_write_tokens: entry.usage.cache_write_tokens.max(0),
        cache_read_tokens: entry.usage.cache_read_tokens.max(0),
        cost_micros,
        price_source,
        upstream_response_id: entry.response_id,
        metadata_json: None,
    }
}

fn proxy_only_row(row: ProxyRequestRow) -> UsageOverviewRow {
    let facts = proxy_facts(&row);
    let model = facts.model.clone().unwrap_or_else(|| "unknown".to_string());
    let usage = TokenUsage {
        input_tokens: row.input_tokens.unwrap_or(0).max(0),
        output_tokens: row.output_tokens.unwrap_or(0).max(0),
        // The proxy stores one combined cache figure; attributing it to reads
        // is the cheaper of the two rates, so an estimate stays a lower bound.
        cache_write_tokens: 0,
        cache_read_tokens: row.cache_tokens.unwrap_or(0).max(0),
    };
    let (cost_micros, price_source) = match upstream_cost_micros(&row) {
        Some(cost) => (cost, Some("upstream".to_string())),
        None => estimated_cost(&model, usage),
    };
    UsageOverviewRow {
        id: row.id,
        source: UsageRowSource::ProxyOnly,
        occurred_at: Some(row.created_at),
        provider: row.platform.clone(),
        model,
        account_id: row.account_id,
        account_name: row.account_name,
        source_label: Some(row.source_label),
        path: facts.path,
        status: facts.status,
        success: facts.success,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cost_micros,
        price_source,
        upstream_response_id: row.upstream_response_id,
        metadata_json: Some(row.metadata_json),
    }
}

/// Price from the local table. `None` cost with no source means the model has
/// no known rate, so the row reads as unpriced rather than free.
fn estimated_cost(model: &str, usage: TokenUsage) -> (i64, Option<String>) {
    match model_pricing::estimate_cost_micros(model, usage) {
        Some(cost) => (cost, Some("estimated".to_string())),
        None => (0, None),
    }
}

fn rfc3339_from_millis(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis).map(|value| value.to_rfc3339())
}
```

若 `chrono::DateTime::from_timestamp_millis` 在本仓库的 chrono 版本下不可用，改用 `chrono::DateTime::<chrono::Utc>::from_timestamp_millis`，或 `chrono::Utc.timestamp_millis_opt(millis).single()`（需 `use chrono::TimeZone`）。编译器会指明。

- [ ] **Step 4: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib usage_overview_service
```

期望：10 passed。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/usage_overview_service.rs src-tauri/src/services/mod.rs
git commit -m "feat: 按上游响应 id 合并会话记录与代理请求"
```

---

### Task 5c: 汇总、四维分组、分页

在 Task 5b 的合并结果之上算汇总与分组，并组装成 `UsageOverview`。

**Files:**
- Modify: `src-tauri/src/services/usage_overview_service.rs`

**Interfaces:**
- Produces:
  - `pub fn summarize(rows: &[UsageOverviewRow]) -> UsageOverviewTotals`
  - `pub fn group_all(rows: &[UsageOverviewRow]) -> UsageOverviewGroups`
  - `pub fn paginate(rows: &[UsageOverviewRow], page: i64, page_size: i64) -> Vec<UsageOverviewRow>`
  - `fn integrity_of(rows: &[UsageOverviewRow], scanned_file_count: i64, truncated: bool, unmatchable_proxy_row_count: i64) -> UsageOverviewIntegrity`（私有，只被 `build_usage_overview` 与测试调用）
  - `pub async fn build_usage_overview(pool, since, page, page_size, window) -> Result<UsageOverview, AppError>`

**关键约束**（spec）：`totals` 与 `groups` 算的是时间窗内**全量**行，不是当前页。分页只影响 `rows`。

- [ ] **Step 1: 写失败测试**

在 `usage_overview_service.rs` 的测试模块追加：

```rust
    fn row_with(
        source: UsageRowSource,
        provider: &str,
        model: &str,
        account: Option<&str>,
        cost: i64,
    ) -> UsageOverviewRow {
        UsageOverviewRow {
            id: format!("{model}-{cost}"),
            source,
            occurred_at: Some("2026-08-19T14:04:50Z".to_string()),
            provider: provider.to_string(),
            model: model.to_string(),
            account_id: account.map(|_| "cred-1".to_string()),
            account_name: account.map(str::to_string),
            source_label: None,
            path: None,
            status: None,
            success: true,
            input_tokens: 100,
            output_tokens: 10,
            cache_write_tokens: 2,
            cache_read_tokens: 3,
            cost_micros: cost,
            price_source: Some("upstream".to_string()),
            upstream_response_id: None,
            metadata_json: None,
        }
    }

    #[test]
    fn totals_add_up_every_row_exactly_once() {
        let rows = vec![
            row_with(UsageRowSource::Matched, "claude", "claude-opus-5", Some("A"), 1_000),
            row_with(UsageRowSource::SessionOnly, "claude", "claude-opus-5", None, 2_000),
            row_with(UsageRowSource::ProxyOnly, "codex", "gpt-5.6-sol", Some("B"), 3_000),
        ];

        let totals = summarize(&rows);

        assert_eq!(totals.request_count, 3);
        assert_eq!(totals.input_tokens, 300);
        assert_eq!(totals.output_tokens, 30);
        assert_eq!(totals.cache_write_tokens, 6);
        assert_eq!(totals.cache_read_tokens, 9);
        assert_eq!(totals.cost_micros, 6_000);
    }

    #[test]
    fn groups_cover_all_four_dimensions() {
        let rows = vec![
            row_with(UsageRowSource::Matched, "claude", "claude-opus-5", Some("A"), 1_000),
            row_with(UsageRowSource::SessionOnly, "claude", "claude-haiku-4-5", None, 2_000),
            row_with(UsageRowSource::ProxyOnly, "codex", "gpt-5.6-sol", Some("B"), 3_000),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_model.len(), 3);
        assert_eq!(groups.by_platform.len(), 2);
        assert_eq!(groups.by_source.len(), 3);
        // Two named accounts plus one bucket for the rows with none.
        assert_eq!(groups.by_account.len(), 3);
    }

    #[test]
    fn account_grouping_buckets_rows_that_never_went_through_the_proxy() {
        // Most merged rows come from transcripts and have no account, so the
        // bucket has to be an explicit, named row rather than a blank label.
        let rows = vec![
            row_with(UsageRowSource::SessionOnly, "claude", "claude-opus-5", None, 2_000),
            row_with(UsageRowSource::SessionOnly, "claude", "claude-opus-5", None, 3_000),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_account.len(), 1);
        assert_eq!(groups.by_account[0].key, "未经代理");
        assert_eq!(groups.by_account[0].totals.request_count, 2);
        assert_eq!(groups.by_account[0].totals.cost_micros, 5_000);
    }

    #[test]
    fn groups_are_ordered_by_cost_so_the_biggest_spend_reads_first() {
        let rows = vec![
            row_with(UsageRowSource::Matched, "claude", "cheap-model", Some("A"), 10),
            row_with(UsageRowSource::Matched, "claude", "pricey-model", Some("A"), 9_000),
        ];

        let groups = group_all(&rows);

        assert_eq!(groups.by_model[0].key, "pricey-model");
    }

    #[test]
    fn source_group_keys_are_human_readable() {
        let rows = vec![
            row_with(UsageRowSource::Matched, "claude", "m", Some("A"), 1),
            row_with(UsageRowSource::SessionOnly, "claude", "m", None, 1),
            row_with(UsageRowSource::ProxyOnly, "codex", "m", Some("B"), 1),
        ];

        let groups = group_all(&rows);
        let keys: Vec<&str> = groups.by_source.iter().map(|row| row.key.as_str()).collect();

        assert!(keys.contains(&"匹配"));
        assert!(keys.contains(&"仅会话"));
        assert!(keys.contains(&"仅代理"));
    }

    #[test]
    fn paging_slices_rows_without_shrinking_the_totals() {
        // The cards answer "what did I spend in this period", so they must not
        // change as the user walks through pages.
        let rows: Vec<UsageOverviewRow> = (0..25)
            .map(|index| {
                row_with(
                    UsageRowSource::SessionOnly,
                    "claude",
                    &format!("model-{index}"),
                    None,
                    100,
                )
            })
            .collect();

        let first = paginate(&rows, 1, 20);
        let second = paginate(&rows, 2, 20);

        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 5);
        assert_eq!(summarize(&rows).cost_micros, 2_500);
    }

    #[test]
    fn a_page_past_the_end_yields_no_rows_rather_than_an_error() {
        let rows = vec![row_with(
            UsageRowSource::SessionOnly,
            "claude",
            "m",
            None,
            1,
        )];

        assert!(paginate(&rows, 99, 20).is_empty());
    }

    #[test]
    fn integrity_counts_unpriced_and_estimated_rows() {
        let mut unpriced = row_with(UsageRowSource::SessionOnly, "codex", "unknown", None, 0);
        unpriced.price_source = None;
        let mut estimated = row_with(UsageRowSource::SessionOnly, "claude", "m", None, 500);
        estimated.price_source = Some("estimated".to_string());
        let upstream = row_with(UsageRowSource::Matched, "claude", "m", Some("A"), 700);

        let integrity = integrity_of(&[unpriced, estimated, upstream], 1_186, false, 4);

        assert_eq!(integrity.unpriced_request_count, 1);
        assert_eq!(integrity.estimated_price_request_count, 1);
        assert_eq!(integrity.scanned_file_count, 1_186);
        assert_eq!(integrity.unmatchable_proxy_row_count, 4);
        assert!(!integrity.truncated);
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib usage_overview_service
```

期望：编译失败——`summarize`、`group_all`、`paginate`、`integrity_of` 都不存在。

- [ ] **Step 3: 实现**

追加到 `usage_overview_service.rs`：

```rust
impl UsageOverviewTotals {
    fn absorb(&mut self, row: &UsageOverviewRow) {
        self.request_count += 1;
        self.input_tokens += row.input_tokens;
        self.output_tokens += row.output_tokens;
        self.cache_write_tokens += row.cache_write_tokens;
        self.cache_read_tokens += row.cache_read_tokens;
        self.cost_micros += row.cost_micros;
    }
}

/// Totals over every row in the window — never over one page, or the summary
/// cards would change as the user pages through the list.
pub fn summarize(rows: &[UsageOverviewRow]) -> UsageOverviewTotals {
    let mut totals = UsageOverviewTotals::default();
    for row in rows {
        totals.absorb(row);
    }
    totals
}

/// Label for the "source" grouping dimension.
fn source_label(source: UsageRowSource) -> &'static str {
    match source {
        UsageRowSource::Matched => "匹配",
        UsageRowSource::SessionOnly => "仅会话",
        UsageRowSource::ProxyOnly => "仅代理",
    }
}

/// Bucket for rows with no owning account. Most merged rows are transcript-only,
/// so this needs a real label rather than an empty cell.
const NO_ACCOUNT_LABEL: &str = "未经代理";

fn group_by<'a, F>(rows: &'a [UsageOverviewRow], key: F) -> Vec<UsageOverviewGroupRow>
where
    F: Fn(&'a UsageOverviewRow) -> String,
{
    let mut buckets: HashMap<String, UsageOverviewTotals> = HashMap::new();
    for row in rows {
        buckets.entry(key(row)).or_default().absorb(row);
    }
    let mut grouped: Vec<UsageOverviewGroupRow> = buckets
        .into_iter()
        .map(|(key, totals)| UsageOverviewGroupRow { key, totals })
        .collect();
    // Highest spend first, then by request count so unpriced groups still
    // order sensibly, then by key for a stable result.
    grouped.sort_by(|left, right| {
        right
            .totals
            .cost_micros
            .cmp(&left.totals.cost_micros)
            .then_with(|| right.totals.request_count.cmp(&left.totals.request_count))
            .then_with(|| left.key.cmp(&right.key))
    });
    grouped
}

/// All four dimensions at once: their cardinality is small (single to double
/// digits), so computing them together avoids a refetch when the user flips the
/// segmented control.
pub fn group_all(rows: &[UsageOverviewRow]) -> UsageOverviewGroups {
    UsageOverviewGroups {
        by_model: group_by(rows, |row| row.model.clone()),
        by_platform: group_by(rows, |row| row.provider.clone()),
        by_account: group_by(rows, |row| {
            row.account_name
                .clone()
                .or_else(|| row.account_id.clone())
                .unwrap_or_else(|| NO_ACCOUNT_LABEL.to_string())
        }),
        by_source: group_by(rows, |row| source_label(row.source).to_string()),
    }
}

/// One page of rows. A page past the end is empty rather than an error: the
/// list shrinks between refreshes as rows age out of the window.
pub fn paginate(rows: &[UsageOverviewRow], page: i64, page_size: i64) -> Vec<UsageOverviewRow> {
    let offset = ((page - 1).max(0) as usize).saturating_mul(page_size.max(1) as usize);
    rows.iter()
        .skip(offset)
        .take(page_size.max(1) as usize)
        .cloned()
        .collect()
}

fn integrity_of(
    rows: &[UsageOverviewRow],
    scanned_file_count: i64,
    truncated: bool,
    unmatchable_proxy_row_count: i64,
) -> UsageOverviewIntegrity {
    UsageOverviewIntegrity {
        scanned_file_count,
        truncated,
        unpriced_request_count: rows
            .iter()
            .filter(|row| row.price_source.is_none())
            .count() as i64,
        estimated_price_request_count: rows
            .iter()
            .filter(|row| row.price_source.as_deref() == Some("estimated"))
            .count() as i64,
        unmatchable_proxy_row_count,
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib usage_overview_service
```

期望：全绿（18 个测试）。

- [ ] **Step 5: 实现顶层入口**

追加：

```rust
/// Assemble the full overview: merge, summarize, group, and page.
///
/// The transcript scan is blocking file IO over a corpus that can reach
/// gigabytes, so it runs on a blocking thread. Warm scans hit the per-file parse
/// cache in [`session_usage_service`].
pub async fn build_usage_overview(
    pool: &SqlitePool,
    since: Option<&str>,
    page: i64,
    page_size: i64,
    window: TimeWindow,
) -> Result<UsageOverview, AppError> {
    let proxy_rows = RoutePoolRepository::list_request_events(pool, since).await?;
    let unmatchable_proxy_row_count = proxy_rows
        .iter()
        .filter(|row| resolve_proxy_response_id(row).is_none())
        .count() as i64;

    let (session_entries, scanned_file_count, truncated) =
        tokio::task::spawn_blocking(move || session_usage_service::collect_session_entries(window))
            .await
            .map_err(|error| AppError::Filesystem {
                code: "filesystem.session_usage_scan_failed",
                message: format!("Failed to scan session usage: {error}"),
                details: None,
                recoverable: true,
            })?;

    let rows = merge_entries(session_entries, proxy_rows);
    let totals = summarize(&rows);
    let groups = group_all(&rows);
    let integrity = integrity_of(
        &rows,
        scanned_file_count,
        truncated,
        unmatchable_proxy_row_count,
    );

    Ok(UsageOverview {
        totals,
        row_count: rows.len() as i64,
        rows: paginate(&rows, page, page_size),
        groups,
        page,
        page_size,
        integrity,
    })
}
```

- [ ] **Step 6: 运行全量 Rust 测试**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test
```

期望：全绿。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/services/usage_overview_service.rs
git commit -m "feat: 用量总览的汇总、四维分组与分页"
```

---

### Task 6: 注册命令

暴露给 Tauri 与 web 两个入口。

**Files:**
- Create: `src-tauri/src/core/usage_overview.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Modify: `src-tauri/src/commands/usage_stats_commands.rs`
- Modify: `src-tauri/src/lib.rs`（import 与 `invoke_handler` 注册）
- Modify: `src-tauri/src/web/handlers/mod.rs`

**Interfaces:**
- Consumes: `usage_overview_service::build_usage_overview`（Task 5c）
- Produces: Tauri 命令 `get_usage_overview(since, page, page_size)`

**为什么另建 `core` 模块**：Tauri 命令与 web 分发器必须行为一致，仓库已有这个约定（`core/usage_stats.rs` 的文件头写明了原因：实现放 core，改动就不会只落在一边）。`parse_window` 已在 `core::usage_stats` 里（`:56-74`），复用它而不是重写。

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/core/usage_overview.rs`：

```rust
//! Shared usage-overview logic for the Tauri commands and the web dispatcher.
//!
//! Mirrors [`crate::core::usage_stats`]: keeping the implementation here means a
//! change cannot land on one surface and be forgotten on the other.

use crate::error::AppError;
use crate::services::usage_overview_service::{self, UsageOverview};
use sqlx::SqlitePool;

const DEFAULT_PAGE: i64 = 1;
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;

/// Merge transcript usage with proxied requests and return one page of the
/// combined list plus window-wide totals and groups.
///
/// `since` is an optional RFC 3339 timestamp, matching `get_route_pool` and
/// `get_session_usage_stats` so the UI can reuse its period selector.
pub async fn get_usage_overview_core(
    pool: &SqlitePool,
    since: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<UsageOverview, AppError> {
    let window = super::usage_stats::parse_window(since.as_deref())?;
    let (page, page_size) = normalize_pagination(page, page_size);
    usage_overview_service::build_usage_overview(
        pool,
        since.as_deref().map(str::trim).filter(|v| !v.is_empty()),
        page,
        page_size,
        window,
    )
    .await
}

/// Clamp paging into a usable range rather than rejecting it: a stale page
/// number from the UI should show an empty page, not an error dialog.
fn normalize_pagination(page: Option<i64>, page_size: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(DEFAULT_PAGE).max(1);
    let page_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    (page, page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_defaults_match_the_previous_request_list() {
        assert_eq!(normalize_pagination(None, None), (1, 20));
    }

    #[test]
    fn pagination_clamps_rather_than_rejecting() {
        // A stale page number or a hand-crafted web request must not error out.
        assert_eq!(normalize_pagination(Some(0), Some(0)), (1, 1));
        assert_eq!(normalize_pagination(Some(-5), Some(9_999)), (1, 100));
    }

    #[tokio::test]
    async fn an_invalid_since_is_rejected_rather_than_widened() {
        // Silently treating a bad timestamp as "all time" would inflate the
        // figures shown for a narrow period.
        let pool = crate::database::create_memory_pool().await.expect("pool");
        crate::database::run_migrations(&pool).await.expect("migrations");

        let error = get_usage_overview_core(&pool, Some("last tuesday".to_string()), None, None)
            .await
            .expect_err("must reject");

        assert!(matches!(
            error,
            AppError::Validation {
                code: "validation.invalid_timestamp",
                ..
            }
        ));
    }
}
```

在 `src-tauri/src/core/mod.rs` 加一行（字母序在 `terminals` 之后）：

```rust
pub mod usage_overview;
```

`core::usage_stats::parse_window` 目前是私有 `fn`，改成 `pub(crate) fn`（在 `src-tauri/src/core/usage_stats.rs:56`）。

- [ ] **Step 2: 运行测试确认失败**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib usage_overview
```

期望：编译失败（`parse_window` 私有，或模块未注册）。改成 `pub(crate)` 后应通过。

- [ ] **Step 3: 注册 Tauri 命令**

在 `src-tauri/src/commands/usage_stats_commands.rs` 追加：

```rust
/// Merge local CLI transcript usage with proxied requests into one deduplicated
/// list, with window-wide totals and per-dimension groups.
#[tauri::command]
pub async fn get_usage_overview(
    state: tauri::State<'_, crate::app_state::AppState>,
    since: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<UsageOverview, ApiError> {
    get_usage_overview_core(&state.pool, since, page, page_size)
        .await
        .map_err(ApiError::from)
}
```

同文件顶部的 import 补上：

```rust
use crate::core::usage_overview::get_usage_overview_core;
use crate::services::usage_overview_service::UsageOverview;
```

在 `src-tauri/src/lib.rs` 找到 `use commands::usage_stats_commands::{...}`（约 `:64-70` 区域，与 `get_session_usage_stats` 同处）加入 `get_usage_overview`，并在 `invoke_handler` 的 `get_session_usage_stats`（`:489`）下一行加：

```rust
            get_usage_overview,
```

- [ ] **Step 4: 注册 web 分发器**

在 `src-tauri/src/web/handlers/mod.rs` 的 `"get_session_usage_stats"` 分支（`:355-362`）之后插入：

```rust
        "get_usage_overview" => {
            let since = optional_string_arg(&args, "since")?;
            let page = optional_i64_arg(&args, "page")?;
            let page_size = optional_i64_arg(&args, "page_size")?;
            to_value(
                get_usage_overview_core(&state.pool, since, page, page_size)
                    .await
                    .map_err(to_error)?,
            )
        }
```

同文件顶部 import 补 `use crate::core::usage_overview::get_usage_overview_core;`（与已有的 `get_session_usage_stats_core` 同处）。

- [ ] **Step 5: 运行全量 Rust 测试并检查两端一致**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test && CARGO_TARGET_DIR=target-codex cargo check --bin ai-switch-server
```

期望：全绿。`cargo check --bin ai-switch-server` 单独跑一遍是因为 web 分发器只在这个 binary 里编译，`cargo test` 未必覆盖。

- [ ] **Step 6: 格式化并提交**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt
cd /d/Repos/xyito/open/ai-switch-task-14
git add src-tauri/src/core/usage_overview.rs src-tauri/src/core/mod.rs \
        src-tauri/src/core/usage_stats.rs \
        src-tauri/src/commands/usage_stats_commands.rs \
        src-tauri/src/lib.rs src-tauri/src/web/handlers/mod.rs
git commit -m "feat: 注册 get_usage_overview 命令"
```

---

### Task 7: 前端类型与 client

**Files:**
- Modify: `src/lib/api/types.ts`（在 `SessionUsageStats` 之后，约 `:541` 处追加）
- Modify: `src/lib/api/client.ts`（在 `getSessionUsageStats` 之后，约 `:175` 处追加）

**Interfaces:**
- Consumes: Task 6 的命令签名
- Produces:
  - `UsageRowSource`、`UsageOverviewRow`、`UsageOverviewTotals`、`UsageOverviewGroupRow`、`UsageOverviewGroups`、`UsageOverviewIntegrity`、`UsageOverview`
  - `getUsageOverview(since, page, pageSize): Promise<UsageOverview>`

Rust 侧用 `#[serde(rename_all = "snake_case")]` 序列化枚举、`#[serde(flatten)]` 摊平 totals，TS 类型要对齐这两点。

- [ ] **Step 1: 追加类型**

在 `src/lib/api/types.ts` 的 `SessionUsageStats` 定义之后追加：

```typescript
/** Where a merged usage row's data came from; also the "source" grouping key. */
export type UsageRowSource = "matched" | "session_only" | "proxy_only";

/** Aggregated counts for one grouping, or for the whole window. */
export type UsageOverviewTotals = {
  request_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  /** USD micros (1 USD = 1_000_000). */
  cost_micros: number;
};

/** One merged request: a transcript entry, a proxied row, or both. */
export type UsageOverviewRow = {
  id: string;
  source: UsageRowSource;
  /** RFC 3339; null when a transcript entry carried no timestamp. */
  occurred_at?: string | null;
  provider: string;
  model: string;
  account_id?: string | null;
  account_name?: string | null;
  source_label?: string | null;
  path?: string | null;
  /** Only ever present on a row that has a proxy side. */
  status?: string | null;
  success: boolean;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  cost_micros: number;
  /**
   * `upstream` when a real upstream price was used, `estimated` when the local
   * price table was, null when the model has no known rate.
   */
  price_source?: "upstream" | "estimated" | null;
  upstream_response_id?: string | null;
  metadata_json?: string | null;
};

/**
 * One group row. The Rust side flattens the totals into this object, so their
 * fields appear inline rather than nested.
 */
export type UsageOverviewGroupRow = UsageOverviewTotals & {
  key: string;
};

export type UsageOverviewGroups = {
  by_model: UsageOverviewGroupRow[];
  by_platform: UsageOverviewGroupRow[];
  by_account: UsageOverviewGroupRow[];
  by_source: UsageOverviewGroupRow[];
};

/** Facts for the data-completeness note beneath the summary cards. */
export type UsageOverviewIntegrity = {
  scanned_file_count: number;
  /** True when the transcript file cap was hit, so the totals are a floor. */
  truncated: boolean;
  unpriced_request_count: number;
  estimated_price_request_count: number;
  /**
   * Proxy rows with no recoverable response id. These could not be merged, so a
   * request may be counted on both sides.
   */
  unmatchable_proxy_row_count: number;
};

/**
 * Local CLI transcript usage merged with proxied requests, deduplicated on the
 * upstream response id. `totals` and `groups` cover the whole window; `rows` is
 * one page.
 */
export type UsageOverview = {
  totals: UsageOverviewTotals;
  rows: UsageOverviewRow[];
  groups: UsageOverviewGroups;
  row_count: number;
  page: number;
  page_size: number;
  integrity: UsageOverviewIntegrity;
};
```

- [ ] **Step 2: 追加 client 函数**

在 `src/lib/api/client.ts` 的 `getSessionUsageStats` 之后追加：

```typescript
/**
 * Local CLI transcript usage merged with proxied requests, deduplicated.
 *
 * `since` is an RFC 3339 timestamp; pass null for the full history.
 */
export function getUsageOverview(
  since: string | null,
  page: number,
  pageSize: number,
): Promise<UsageOverview> {
  return invoke("get_usage_overview", {
    since: since ?? null,
    page,
    page_size: pageSize,
  });
}
```

同文件顶部的类型 import 补上 `UsageOverview`（与已有的 `SessionUsageStats` 同处，约 `:51`）。

- [ ] **Step 3: 类型检查**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14 && pnpm typecheck
```

期望：通过。

- [ ] **Step 4: 提交**

```bash
git add src/lib/api/types.ts src/lib/api/client.ts
git commit -m "feat: 新增用量总览的 API 类型与调用"
```

---

### Task 8: 用量总览面板组件

**Files:**
- Create: `src/components/accounts/UsageOverviewPanel.tsx`
- Test: `tests/UsageOverviewPanel.test.tsx`

**Interfaces:**
- Consumes:
  - `getUsageOverview`（Task 7）
  - `formatCompactCount`、`formatExactCount`（Task 2）
- Produces: `export function UsageOverviewPanel(): JSX.Element` —— 无 props，自带全部 state

**从 AccountsScreen 迁入的东西**（原位置见 spec）：期间选择器（`routeStatsPeriods` + `routeStatsSince`，`AccountsScreen.tsx:297-302`、`:321-340`）、`formatCostMicros`（`:474-489`）、`formatUsageTime`（`:342-348`，**AccountsScreen 里要保留一份**，实时日志弹窗在用）。

`formatApiError` 也在 AccountsScreen 里，新组件需要它——查一下它是否已导出；若没有，在新组件里写一个等价的本地实现，不要为了复用去改 AccountsScreen 的导出（那会扩大改动面）。

**界面结构**（spec「顶部卡片」「分段器」「请求列表」）：

1. 标题「用量总览」+ 副标题 + 期间分段器（当日 / 本周 / 本月 / 累计）
2. 5 张汇总卡：请求 / 输入 / 输出 / 缓存 / 费用
3. 一行数据完整性说明
4. 分组分段器（模型 / 平台 / 账号 / 来源），**默认整块收起**
5. 合并列表：时间 / 模型 / Token / 费用 / 账号 / 来源 + 详情按钮
6. 面板内分页

标题文案固定为「用量总览」（Task 9 的测试断言依赖它）。副标题**不绑定平台**——顶部数字跨 provider，原来的「统计当前 Codex 的历史路由请求」已不成立。写成类似「合并本机 CLI 会话记录与代理请求，同一请求只计一次」。

- [ ] **Step 1: 写失败测试**

创建 `tests/UsageOverviewPanel.test.tsx`：

```tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getUsageOverview } from "../src/lib/api/client";
import { createQueryClient } from "../src/lib/query/queryClient";
import { UsageOverviewPanel } from "../src/components/accounts/UsageOverviewPanel";
import type { UsageOverview, UsageOverviewRow } from "../src/lib/api/types";

vi.mock("../src/lib/api/client", () => ({
  getUsageOverview: vi.fn(),
}));

function rowFixture(overrides: Partial<UsageOverviewRow> = {}): UsageOverviewRow {
  return {
    id: "row-1",
    source: "matched",
    occurred_at: "2026-08-19T14:04:50Z",
    provider: "claude",
    model: "claude-opus-5",
    account_id: "cred-1",
    account_name: "Team Account",
    source_label: "route_proxy",
    path: "/v1/messages",
    status: "200",
    success: true,
    input_tokens: 120,
    output_tokens: 30,
    cache_write_tokens: 10,
    cache_read_tokens: 40,
    cost_micros: 4_200,
    price_source: "upstream",
    upstream_response_id: "msg_a",
    metadata_json: null,
    ...overrides,
  };
}

function overviewFixture(overrides: Partial<UsageOverview> = {}): UsageOverview {
  return {
    totals: {
      request_count: 11_254,
      input_tokens: 5_584_802_591,
      output_tokens: 129_897_022,
      cache_write_tokens: 318_626_507,
      cache_read_tokens: 19_115_772_272,
      cost_micros: 16_248_905_925,
    },
    rows: [rowFixture()],
    groups: {
      // Deliberately a different model id from rowFixture's `claude-opus-5`:
      // the collapse test asserts this text is absent before the user clicks,
      // which only means something if the row list cannot supply it.
      by_model: [
        {
          key: "claude-haiku-4-5",
          request_count: 152,
          input_tokens: 1_000_000,
          output_tokens: 2_000,
          cache_write_tokens: 0,
          cache_read_tokens: 0,
          cost_micros: 3_357_030_000,
        },
      ],
      by_platform: [
        {
          key: "claude",
          request_count: 152,
          input_tokens: 1_000_000,
          output_tokens: 2_000,
          cache_write_tokens: 0,
          cache_read_tokens: 0,
          cost_micros: 3_357_030_000,
        },
      ],
      by_account: [
        {
          key: "未经代理",
          request_count: 100,
          input_tokens: 500_000,
          output_tokens: 1_000,
          cache_write_tokens: 0,
          cache_read_tokens: 0,
          cost_micros: 1_000_000,
        },
      ],
      by_source: [
        {
          key: "匹配",
          request_count: 152,
          input_tokens: 1_000_000,
          output_tokens: 2_000,
          cache_write_tokens: 0,
          cache_read_tokens: 0,
          cost_micros: 3_357_030_000,
        },
      ],
    },
    row_count: 1,
    page: 1,
    page_size: 20,
    integrity: {
      scanned_file_count: 1_186,
      truncated: false,
      unpriced_request_count: 3,
      estimated_price_request_count: 12,
      unmatchable_proxy_row_count: 5,
    },
    ...overrides,
  };
}

function renderPanel() {
  return render(
    <QueryClientProvider client={createQueryClient()}>
      <UsageOverviewPanel />
    </QueryClientProvider>,
  );
}

describe("UsageOverviewPanel", () => {
  beforeEach(() => {
    vi.mocked(getUsageOverview).mockReset();
    vi.mocked(getUsageOverview).mockResolvedValue(overviewFixture());
  });

  it("renders one set of totals with 万/百万/亿 units and the exact figure in a tooltip", async () => {
    renderPanel();

    // 11,254 requests: over the 万 threshold, so 1.1万 with the exact count on
    // hover. The point of the test is that there is ONE set of numbers now.
    expect(await screen.findByText("1.1万")).toBeInTheDocument();
    expect(screen.getByTitle("11,254")).toBeInTheDocument();
    // 5,584,802,591 input tokens.
    expect(screen.getByText("55.85亿")).toBeInTheDocument();
    expect(screen.getByTitle("5,584,802,591")).toBeInTheDocument();
    // Cost keeps a currency format rather than a 万/亿 unit.
    expect(screen.getByText("$16,248.91")).toBeInTheDocument();
  });

  it("keeps the grouping table collapsed until a dimension is clicked", async () => {
    renderPanel();

    await screen.findByText("1.1万");
    // The group row's model id differs from the list row's, so its absence here
    // proves the group table is genuinely not rendered rather than merely
    // duplicating text the list already shows.
    expect(screen.queryByText("claude-haiku-4-5")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "模型" }));

    expect(await screen.findByText("claude-haiku-4-5")).toBeInTheDocument();
  });

  it("collapses again when the active dimension is clicked a second time", async () => {
    renderPanel();
    await screen.findByText("1.1万");

    await userEvent.click(screen.getByRole("button", { name: "模型" }));
    expect(await screen.findByText("claude-haiku-4-5")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "模型" }));

    expect(screen.queryByText("claude-haiku-4-5")).not.toBeInTheDocument();
  });

  it("switches the grouping dimension without refetching", async () => {
    renderPanel();
    await screen.findByText("1.1万");
    const callsBefore = vi.mocked(getUsageOverview).mock.calls.length;

    await userEvent.click(screen.getByRole("button", { name: "模型" }));
    await userEvent.click(screen.getByRole("button", { name: "账号" }));

    expect(await screen.findByText("未经代理")).toBeInTheDocument();
    // All four dimensions arrive in one response, so flipping the control is
    // free — a refetch here would make the segmented control feel laggy.
    expect(vi.mocked(getUsageOverview).mock.calls.length).toBe(callsBefore);
  });

  it("labels each row with its source", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({ id: "a", source: "matched" }),
          rowFixture({ id: "b", source: "session_only", account_name: null }),
          rowFixture({ id: "c", source: "proxy_only" }),
        ],
        row_count: 3,
      }),
    );

    renderPanel();

    expect(await screen.findByText("匹配")).toBeInTheDocument();
    expect(screen.getByText("仅会话")).toBeInTheDocument();
    expect(screen.getByText("仅代理")).toBeInTheDocument();
  });

  it("shows a status chip only on a failed row", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [
          rowFixture({ id: "ok", status: "200", success: true }),
          rowFixture({ id: "bad", status: "401", success: false }),
        ],
        row_count: 2,
      }),
    );

    renderPanel();

    // A transcript row has no HTTP status at all, so a permanent status column
    // would be mostly blank; only failures earn a chip.
    expect(await screen.findByText("401")).toBeInTheDocument();
    expect(screen.queryByText("200")).not.toBeInTheDocument();
  });

  it("marks rows that never went through the proxy", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({
        rows: [rowFixture({ source: "session_only", account_name: null, account_id: null })],
        // Drop the group fixture so 未经代理 can only come from the row.
        groups: {
          by_model: [],
          by_platform: [],
          by_account: [],
          by_source: [],
        },
      }),
    );

    renderPanel();

    expect(await screen.findByText("未经代理")).toBeInTheDocument();
  });

  it("states how complete the totals are", async () => {
    renderPanel();

    // The semantics are "my total spend", so anything that makes the figure a
    // floor rather than an exact number has to be said out loud.
    expect(await screen.findByText(/已扫描 1,186 个会话文件/)).toBeInTheDocument();
    expect(screen.getByText(/3 个请求的模型没有价格数据/)).toBeInTheDocument();
    expect(screen.getByText(/5 条代理记录无法与会话记录匹配/)).toBeInTheDocument();
  });

  it("requests the selected period", async () => {
    renderPanel();
    await screen.findByText("1.1万");

    await userEvent.click(screen.getByRole("button", { name: "累计" }));

    await waitFor(() =>
      expect(getUsageOverview).toHaveBeenLastCalledWith(null, 1, 20),
    );
  });

  it("pages within the panel", async () => {
    vi.mocked(getUsageOverview).mockResolvedValue(
      overviewFixture({ row_count: 42, page: 1, page_size: 20 }),
    );

    renderPanel();
    expect(await screen.findByText("1/3")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "下一页" }));

    await waitFor(() =>
      expect(getUsageOverview).toHaveBeenLastCalledWith(expect.any(String), 2, 20),
    );
  });

  it("reports a failure instead of rendering zeros", async () => {
    // Zeros would read as "you spent nothing", which is a different and wrong
    // statement from "the figure could not be loaded".
    vi.mocked(getUsageOverview).mockRejectedValue(new Error("scan failed"));
    renderPanel();

    expect(await screen.findByRole("alert")).toHaveTextContent(/scan failed|读取用量失败/);
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14
pnpm vitest run tests/UsageOverviewPanel.test.tsx
```

期望：FAIL，无法解析 `../src/components/accounts/UsageOverviewPanel`。

- [ ] **Step 3: 实现组件**

创建 `src/components/accounts/UsageOverviewPanel.tsx`。要点：

- 四个期间按钮沿用 `routeStatsPeriods` 的键与文案（当日 / 本周 / 本月 / 累计）；`since` 计算逻辑照搬 `AccountsScreen.tsx:321-340` 的 `routeStatsSince`（周起始为周一，`all` 返回 `null`）。
- `useQuery({ queryKey: ["usage-overview", since, page, pageSize], queryFn: () => getUsageOverview(since, page, pageSize), placeholderData: keepPreviousData, refetchInterval: usageOverviewRefreshMs })`。刷新间隔常量先设 `10_000`，并在其上方留注释说明这是待实测的初值（见 Task 10）。
- 分组维度 state：`useState<"model" | "platform" | "account" | "source" | null>(null)`，`null` 即收起。四个按钮文案：模型 / 平台 / 账号 / 来源。点击已选中的维度则收起（回到 `null`）。
- 汇总卡 5 张，每张 `title={formatExactCount(value)}` + `{formatCompactCount(value)}`；费用卡用 `formatCostMicros`，`title` 放 6 位小数的精确美元值。
- 缓存卡显示 `cache_write_tokens + cache_read_tokens`，`title` 写明写入/读取拆分。**注意**：请求数卡的 `title` 是 `formatExactCount(11_254)` = `"11,254"`，测试用 `getByTitle("11,254")` 定位它，所以缓存卡的 title 不能凑巧也是这个值（fixture 里不会）。
- 来源徽标映射：`matched → 匹配`、`session_only → 仅会话`、`proxy_only → 仅代理`。
- 账号列：`row.account_name ?? row.account_id ?? "未经代理"`。与分组表里无账号那一栏用同一个字面量，保持一致。
- 状态 chip：仅 `!row.success && row.status` 时渲染，红色样式。
- 分页：`Math.max(1, Math.ceil(row_count / page_size))`，按钮 `aria-label` 用「上一页」「下一页」，页码文本 `{page}/{pageCount}`。
- 期间切换时把 `page` 重置为 1。
- 数据完整性说明按 `integrity` 组装，各条件独立判断，无内容时整行不渲染。测试断言的三段文案分别是：
  - `已扫描 {scanned_file_count.toLocaleString("en-US")} 个会话文件`（无条件，只要有数据）
  - `其中 {unpriced_request_count} 个请求的模型没有价格数据，未计入费用`（`> 0` 时）
  - `{unmatchable_proxy_row_count} 条代理记录无法与会话记录匹配，对应请求可能被重复计入`（`> 0` 时）
  - 另外两条无测试断言但要有：`truncated` 为真时提示总数不完整；`estimated_price_request_count > 0` 时说明部分费用按本地价格表估算。
- 错误分支用 `role="alert"`，消息优先取 `error.message`，兜底「读取用量失败」。

`formatCostMicros` 复制 `AccountsScreen.tsx:474-489` 的实现（含小额多位小数分支）。

- [ ] **Step 4: 运行测试确认通过**

```bash
pnpm vitest run tests/UsageOverviewPanel.test.tsx
```

期望：全绿。

- [ ] **Step 5: 类型检查并提交**

```bash
pnpm typecheck
git add src/components/accounts/UsageOverviewPanel.tsx tests/UsageOverviewPanel.test.tsx
git commit -m "feat: 新增用量总览面板"
```

---

### Task 9: 接入 AccountsScreen 并清理旧统计代码

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`（删除统计实现，接入新面板）
- Modify: `tests/AccountsScreen.test.tsx`（删除已迁走的统计测试）

**要删的东西**（行号基于改动前的文件，删除时从后往前做以免行号漂移）：

| 内容 | 行号 |
| --- | --- |
| 统计面板 JSX | 4699-4967 |
| 状态栏里的统计分页分支 | 5716-5736（保留 `else` 分支的账号分页，去掉三元） |
| `routeStats` / `costTotal` / `sessionUsage` / `sessionTotals` / `requestRowCount` / `resolvedRequestPage` / `resolvedRequestPageSize` / `requestPageCount` | 2840-2850 |
| `requestPage` 钳制 effect | 2740-2752 |
| `expandedRequestId` 清理 effect | 2641-2643 |
| `sessionUsageQuery` 与其注释 | 2482-2492 |
| `statsPeriod` / `requestPage` / `expandedRequestId` state | 2053-2055 |
| `statsSince` useMemo | 2170 |
| `selectStatsPeriod` | 4003-4006 |
| `routeStatsPeriods` / `routeStatsPageSize` / `routeStatsRefreshMs` / `sessionUsageRefreshMs` / `RouteStatsPeriod` / `routeStatsSince` | 297-302、311-317、319、321-340 |
| `ParsedUsageMetadata` / `metadataField` / `optionalMetadataField` / `parseUsageMetadata` / `formatUsageCount` / `formatUsageTotalTokens` / `usageTokenTooltip` / `formatUsagePrice` / `formatCostMicros` / `formatTokenCount` / `RouteRequestDetail` | 365-500、502-577 |
| `getSessionUsageStats` import | 75 |

**绝对不要删**（已核对调用点）：

- `formatUsageTime`（342-348）—— 实时日志弹窗在 6072 行用。
- `prettyJsonOrText`（1816-1822）—— 实时日志（359）、凭证失败提示（1874）、模型测试面板（4667、4673）在用。
- `LiveLogStage`（354-363）、`liveLogStagesIdentical`（350-352）—— 只服务实时日志弹窗，名字像统计但无关。
- `credentialRequestStats`（1288-1303）—— 账号列表的每凭证元数据。
- `RoutePoolUsageLog` type import（123）—— 若删完统计代码后确实无引用，编译器/eslint 会提示，届时再删。

**要改的东西**：

- `routePoolQuery`（2476-2481）的 queryKey 退回 `["route-pool", activePlatform]`，queryFn 改为 `getRoutePool(activePlatform, null, null, null)`，去掉 `refetchInterval`。
- 模型测试的乐观缓存写入（3253-3260）与回滚（3227-3228）里的 queryKey 同步改成 `["route-pool", activePlatform]`。
- `selectAccountView`（4008-4016）里 `view === "stats"` 时的 `routePoolQuery.refetch()` 可以去掉——面板自己拉数据了。
- 平台切换 effect（2635-2639）里的 `setRequestPage(1)` 去掉，其余保留。
- 统计面板渲染位置（原 4699）换成 `{statsOpen && <UsageOverviewPanel />}`。

- [ ] **Step 1: 先删测试，确认红**

在 `tests/AccountsScreen.test.tsx` 删除三个已迁走的统计测试：

- `renders filtered route request statistics, expands request details, and paginates request rows`（2928-3135）
- `renders local session usage alongside route statistics`（3137-3203）
- `shows sub-cent route costs instead of rounding them to $0.00`（3205-3235）
- `auto refreshes route statistics only while the panel is open`（3237-3282）

同时删除 `getSessionUsageStats` 的 import（16）、mock 声明（77）、`beforeEach` 里的 reset 与默认返回（329-346）。

保留：
- `switches between pooled, unpooled, and statistics segments with scoped actions`（1272-1303）—— 它断言切到统计后账号列表消失，仍然有效。但 1299 行的 `expect(await screen.findByText("请求统计")).toBeInTheDocument()` 要改成 `"用量总览"`。
- `tests the credential pool route …` 的 3325 行负向断言，同样把 `"请求统计"` 换成 `"用量总览"`。

新增一个测试确认面板被挂载且账号列表让位：

```tsx
  it("mounts the usage overview panel in the statistics view", async () => {
    vi.mocked(getRoutePool).mockResolvedValue({
      platform: "codex",
      account_ids: [],
      stats: statsFixture(),
    });

    renderScreen();
    await screen.findByText("筛选：");

    await selectAccountView("统计");

    // The panel owns its own data; AccountsScreen only toggles it.
    expect(await screen.findByText("用量总览")).toBeInTheDocument();
    expect(screen.queryByText("筛选：")).not.toBeInTheDocument();
  });
```

因为 `UsageOverviewPanel` 会调用 `getUsageOverview`，在 `tests/AccountsScreen.test.tsx` 的 `vi.mock("../src/lib/api/client", ...)` 里补 `getUsageOverview: vi.fn()`，并在 `beforeEach` 里给它一个空的默认返回（照现有 `getSessionUsageStats` 默认值的写法，避免查询悬挂）。

```bash
pnpm vitest run tests/AccountsScreen.test.tsx
```

期望：新测试 FAIL（`用量总览` 找不到），其余通过。

- [ ] **Step 2: 改 AccountsScreen**

按上面的清单删除与修改。加 import：

```typescript
import { UsageOverviewPanel } from "../components/accounts/UsageOverviewPanel";
```

- [ ] **Step 3: 运行测试确认通过**

```bash
pnpm vitest run tests/AccountsScreen.test.tsx
```

期望：全绿。

- [ ] **Step 4: 全量前端测试 + 类型检查**

```bash
pnpm test:run && pnpm typecheck
```

期望：全绿。若 `AccountsScreen.tsx` 有未使用的 import 或变量残留，`pnpm typecheck` 会报出来——逐个删掉，不要留 `_` 前缀的占位变量。

- [ ] **Step 5: 确认删除范围没有误伤**

```bash
grep -n 'formatUsageTime\|prettyJsonOrText\|LiveLogStage\|liveLogStagesIdentical' src/screens/AccountsScreen.tsx
```

期望：这四个都还在，且各自的调用点还在（实时日志弹窗、凭证失败提示、模型测试面板）。

```bash
grep -n 'getSessionUsageStats\|routeStatsSince\|parseUsageMetadata\|formatTokenCount' src/screens/AccountsScreen.tsx
```

期望：无输出（都已迁走或删除）。

- [ ] **Step 6: 提交**

```bash
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "refactor: 统计视图改用独立的用量总览面板"
```

---

### Task 10: 实测刷新间隔与端到端验证

设计里刷新间隔按 10 秒暂定，因为当前 worktree 无法测量暖扫描耗时。这一步补上，并在真实应用里确认改动可用。

**Files:**
- Modify: `src/components/accounts/UsageOverviewPanel.tsx`（可能调整刷新间隔常量）

- [ ] **Step 1: 测量暖扫描耗时**

```bash
cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test --lib real_corpus -- --ignored --nocapture
```

记下输出里的 `warm scan` 秒数。这个数字是刷新间隔的下界参考——间隔必须显著大于它，否则刷新会持续占用一个阻塞线程。

- [ ] **Step 2: 按实测结果定间隔**

判断标准：

- 暖扫描 < 1 秒 → 保持 10 秒。
- 1-3 秒 → 改成 30 秒。
- \> 3 秒 → 改成 60 秒（与被替换的 `sessionUsageRefreshMs` 一致），并在常量上方注释记录实测值与机器规模（文件数）。

修改 `UsageOverviewPanel.tsx` 里的常量，并把注释里「待实测」改成实测结论。

- [ ] **Step 3: 启动应用验证**

```bash
cd /d/Repos/xyito/open/ai-switch-task-14 && pnpm tauri:dev
```

注意 `pnpm tauri:dev` 用的是 `src-tauri/target/`（不是 `target-codex`），这是 AGENTS.md 规定的分工。

在应用里逐项确认：

1. 切到「统计」，面板出现，顶部 5 张卡有数字。
2. 数字带万/百万/亿单位，悬浮显示精确值。
3. 列表里能看到三种来源徽标（若本机数据只有其中一两种，至少确认存在的那些渲染正常）。
4. 点「模型」展开分组表，再点一次收起；切到「账号」不产生网络请求（DevTools Network 面板确认）。
5. 分页在面板内，翻页有效。
6. 切到「算力池」再切回来，数字仍在。
7. 切换平台（Codex ↔ Claude），数字不变（跨 provider 语义），且不触发重新请求。

- [ ] **Step 4: 确认数据合理性**

对比改动前后的数字量级：合并后的请求数应**大于**原「路由请求统计」的请求数（多了会话独有的部分），且**小于或等于**原「路由请求统计」+「本机会话用量」两者之和（重叠区被去重了）。

若合并后的请求数等于两者之和，说明去重完全没生效——检查 `upstream_response_id` 是否真的写进去了：

```bash
sqlite3 ~/.ai-switch/ai-switch-dev.db "SELECT COUNT(*) total, COUNT(upstream_response_id) with_id FROM usage_events WHERE metric_type='request';"
```

新产生的行应该有 id。历史行为 NULL 属正常（靠预览解析兜住）。

- [ ] **Step 5: 若调整了间隔则提交**

```bash
git add src/components/accounts/UsageOverviewPanel.tsx
git commit -m "perf: 按实测暖扫描耗时确定用量总览刷新间隔"
```

---

## 收尾检查

- [ ] `pnpm typecheck` 通过
- [ ] `pnpm test:run` 通过
- [ ] `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo test` 通过
- [ ] `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo fmt --check` 通过
- [ ] `cd src-tauri && CARGO_TARGET_DIR=target-codex cargo check --bin ai-switch-server` 通过
- [ ] `git status` 干净，且没有新建 `target-*` 目录（`ls -d src-tauri/target*` 只应有 `target` 与 `target-codex`）
- [ ] 迁移文件只新增未修改：`git diff main --stat -- src-tauri/migrations/` 只应显示一个新文件
