# 生图插件实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不改变现有平台、普通分组和双成员模型的前提下，实现可管理会话的生图插件、OpenAI/Gemini 上游路由，以及 SaaS 图片接口和按张计费。

**Architecture:** 核心代理继续以平台激活组或 SaaS Key 绑定组作为候选范围，模型映射增加图片能力，接口格式负责协议选择。`imagegen` 子系统负责会话、任务和资产；SaaS 复用同一图片执行入口并增加媒体价格快照、预占和结算。

**Tech Stack:** Rust、Axum、SQLx/SQLite、Reqwest、Tauri、React、TypeScript、TanStack Query、Vitest

**Spec:** `docs/superpowers/specs/2026-09-10-image-generation-plugin-design.md`

**Implementation note:** 本次按第一阶段落地：文生图、会话/资产管理、OpenAI Images 与 Responses/Gemini 桥接、SaaS generations 按张计费。图片编辑和尺寸/质量差异定价未包含，详见设计文档的实施范围说明。

## Global Constraints

- 不新增生图平台、`provider` 字段、成员类型或跨平台分组。
- 普通分组成员仍只有 API 成员和官方凭证，且平台必须与分组一致。
- API 成员按 `interface_format` 路由；官方凭证按平台内置适配器路由。
- 生图插件只消费所选平台当前激活组；SaaS 只消费 Key 固定绑定组。
- Rust 验证统一使用 `src-tauri/target-codex/`，在 `src-tauri` 执行时设置 `CARGO_TARGET_DIR=target-codex`。
- 数据库迁移必须加入 `src-tauri/src/database/migration_checksums.txt`，并保持 LF。

---

### Task 1: 图片模型能力与代理路径

**Files:**
- Modify: `src-tauri/src/models/route_credential.rs`
- Modify: `src-tauri/src/services/route_model_capability.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Modify: `src/lib/api/types.ts`
- Test: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_model_capability.rs`

**Interfaces:**
- Produces: `ModelMapping.capabilities: Vec<String>`；图片路径识别、能力过滤及 OpenAI Images 直接转发。

- [ ] 写失败测试：旧映射默认不具备图片能力，显式 `image.generate` 映射可被图片目录识别。
- [ ] 写失败测试：`/v1/images/generations` 和 `/v1/images/edits` 属于共享模型 API 路径并保持上游路径。
- [ ] 执行目标 Rust 测试并确认失败。
- [ ] 为模型映射增加向后兼容的 `capabilities`，实现图片能力目录和请求候选过滤。
- [ ] 让 `openai` API 成员转发 Images 路径；官方/Responses 路径预留显式错误而非误发文本接口。
- [ ] 执行目标 Rust 测试并确认通过。

### Task 2: 会话、消息与资产仓储

**Files:**
- Create: `src-tauri/migrations/202609100001_image_generation.sql`
- Modify: `src-tauri/src/database/migration_checksums.txt`
- Create: `src-tauri/src/imagegen/mod.rs`
- Create: `src-tauri/src/imagegen/models.rs`
- Create: `src-tauri/src/imagegen/repository.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/paths.rs`
- Test: `src-tauri/src/imagegen/repository.rs`
- Test: `src-tauri/src/database/mod.rs`

**Interfaces:**
- Produces: `ImageSession`、`ImageMessage`、`ImageAsset` 及其 CRUD；`AppPaths.imagegen_dir`。

- [ ] 写失败测试：迁移创建三张表并保证会话级级联删除。
- [ ] 写失败测试：创建、改名、列出、归档会话及写入消息/资产。
- [ ] 执行目标 Rust 测试并确认失败。
- [ ] 添加迁移、校验和及 `imagegen` 模型和仓储实现。
- [ ] 为应用数据目录增加私有 `imagegen` 目录。
- [ ] 执行迁移与仓储测试并确认通过。

### Task 3: 图片执行服务

**Files:**
- Create: `src-tauri/src/imagegen/protocol.rs`
- Create: `src-tauri/src/imagegen/service.rs`
- Create: `src-tauri/src/imagegen/storage.rs`
- Modify: `src-tauri/src/imagegen/mod.rs`
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/imagegen/protocol.rs`
- Test: `src-tauri/src/imagegen/service.rs`
- Test: `src-tauri/src/imagegen/storage.rs`

**Interfaces:**
- Consumes: 平台激活组、模型能力和现有代理候选。
- Produces: `ImageGenerationService::generate`、统一 OpenAI Images/Gemini 图片结果和安全资产落盘。

- [ ] 写失败测试：OpenAI `b64_json` 和 Gemini inline data 转换为统一图片结果。
- [ ] 写失败测试：只从激活组和图片能力模型选择成员。
- [ ] 写失败测试：资产文件名、MIME、大小和哈希校验。
- [ ] 执行目标 Rust 测试并确认失败。
- [ ] 实现协议转换、候选调度、上游调用、结果解析和本地落盘。
- [ ] 将请求结果写入会话、消息、资产和现有用量事件。
- [ ] 执行目标 Rust 测试并确认通过。

### Task 4: Tauri 与 Web 图片 API

**Files:**
- Create: `src-tauri/src/imagegen/commands.rs`
- Create: `src-tauri/src/imagegen/transport.rs`
- Modify: `src-tauri/src/imagegen/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Test: `src-tauri/src/imagegen/transport.rs`
- Test: `src-tauri/src/web/router.rs`

**Interfaces:**
- Produces: 会话 CRUD、生成命令、`/api/imagegen/assets/:id` 鉴权读取。

- [ ] 写失败测试：命令和 Web 调度返回会话、消息及资产。
- [ ] 写失败测试：未授权资产请求被拒绝，合法资产带正确 MIME。
- [ ] 执行目标 Rust 测试并确认失败。
- [ ] 注册 Tauri 命令和受主 token 保护的 Web 命令。
- [ ] 注册资产读取路由并执行所有权与路径边界校验。
- [ ] 执行目标 Rust 测试并确认通过。

### Task 5: 前端生图插件

**Files:**
- Create: `src/imagegen/types.ts`
- Create: `src/imagegen/api.ts`
- Create: `src/imagegen/ImageGenerationScreen.tsx`
- Create: `src/imagegen/imagegen.css`
- Create: `src/imagegen/ImageGenerationScreen.test.tsx`
- Modify: `src/lib/api/client.ts`
- Modify: `src/lib/api/types.ts`
- Modify: `src/components/layout/AppLayout.tsx`
- Modify: `src/App.tsx`
- Modify: `src/lib/i18n.tsx`

**Interfaces:**
- Consumes: Task 4 命令。
- Produces: 导航入口、会话列表、提示编辑器、平台/模型选择、生成状态和图片结果。

- [ ] 写失败组件测试：创建会话、选择 Codex/Gemini、显示激活组图片模型和提交生成。
- [ ] 执行目标 Vitest 并确认失败。
- [ ] 实现类型、API、页面、样式、导航和中英文文案。
- [ ] 覆盖空激活组、无图片模型、失败和加载状态。
- [ ] 执行目标 Vitest、`pnpm typecheck` 并确认通过。

### Task 6: SaaS 图片价格与端点

**Files:**
- Create: `src-tauri/src/saas/migrations/0005_image_billing.sql`
- Modify: `src-tauri/src/saas/repository/mod.rs`
- Modify: `src-tauri/src/saas/billing/pricing.rs`
- Modify: `src-tauri/src/saas/billing/mod.rs`
- Modify: `src-tauri/src/saas/domain/groups.rs`
- Modify: `src-tauri/src/saas/proxy/mod.rs`
- Modify: `src-tauri/src/saas/proxy/usage.rs`
- Modify: `src-tauri/src/saas/proxy/tests.rs`
- Modify: `src/saas/types.ts`
- Modify: `src/saas/admin/Groups.tsx`

**Interfaces:**
- Consumes: Task 3 图片执行服务和普通核心组。
- Produces: 图片价格快照、按张预占/结算、`/v1/images/generations` 与 `/v1/images/edits`。

- [ ] 写失败测试：按数量、尺寸、质量和倍率计算预占金额。
- [ ] 写失败测试：成功、部分成功、失败退款、未知结果待审及幂等重放。
- [ ] 写失败测试：SaaS Key 只能使用绑定组中的图片模型和成员。
- [ ] 执行目标 Rust 测试并确认失败。
- [ ] 扩展 SaaS 迁移、模型配置、计费快照和聚合用量。
- [ ] 接入图片兼容端点和 Task 3 执行服务。
- [ ] 扩展 SaaS 管理页的图片操作、价格与请求限制配置。
- [ ] 执行目标 Rust/Vitest 测试并确认通过。

### Task 7: 集成与回归验证

**Files:**
- Modify: `docs/superpowers/specs/2026-09-10-image-generation-plugin-design.md`（仅在实现偏差需要记录时）
- Test: repository-wide affected suites

**Interfaces:**
- Consumes: Tasks 1-6。
- Produces: 可交付的完整生图插件和 SaaS 生图链路。

- [ ] 运行 `pnpm typecheck`。
- [ ] 运行 `pnpm test:run`。
- [ ] 在 `src-tauri` 以 `CARGO_TARGET_DIR=target-codex cargo fmt --check` 验证格式。
- [ ] 在 `src-tauri` 以 `CARGO_TARGET_DIR=target-codex cargo check` 验证桌面构建。
- [ ] 在 `src-tauri` 以 `CARGO_TARGET_DIR=target-codex cargo test` 运行 Rust 测试。
- [ ] 检查 `git diff --check`、迁移校验和、工作区产物和最终状态。
