# 算力池精确模式实现计划

对应设计：`docs/superpowers/specs/2026-09-06-route-pool-precise-model-mode-design.md`

每个任务先写测试再写实现，任务结束时 `CARGO_TARGET_DIR=target-codex cargo test`（工作目录 `src-tauri`）对应模块能过。

## 1. 模式模块 `route_pool_model_mode.rs`

- `PoolModelMode`（`Aggregate` / `Precise`，`parse` 宽容、`as_str`）、`OFFICIAL_MODEL_PREFIX`。
- `account_model_prefix(display_name, id)`、`assign_member_prefixes(&[(id, display_name)])`、`accepted_prefixes(display_name, id)`、`split_prefixed_model(model)`。
- 测试：空格/`/`/`:` 换 `-`、中文保留、连续 `-` 压缩、32 字符截断、空名回退 `acct-{id8}`、保留字 `official` 被挤成 `official-{id6}`、同名两边都加后缀、`accepted_prefixes` 同时收 `base` 与 `base-{id6}`、`split_prefixed_model` 按第一个 `/` 拆且拒绝空段。
- 在 `services/mod.rs` 注册。

## 2. 迁移与仓储

- `migrations/202609060001_route_pool_model_modes.sql`（先确认 main 未占用该号）。
- `RoutePoolRepository::model_mode` / `save_model_mode`：缺行与脏值都回 `Aggregate`，`ON CONFLICT(platform)` upsert。
- `RoutePoolState.model_mode` 字段 + `RoutePoolService::get` 带出；`SetRoutePoolModelModeInput`。
- 测试：默认值、往返、脏值回落。

## 3. 目录生成改造

- `ModelCatalogMember` / `CatalogMemberInput` / `catalog_members`；`advertised_model_catalog_entries(platform, members, mode)`；`advertised_model_ids`、`codex_model_catalog_payload` 跟签名。
- 聚合模式分支必须让现有断言原样通过（只改造它们的构造方式）。
- 精确模式：API 成员每条 id 加 `{prefix}/`；官方成员先剔 `is_synthetic_route_alias` 再以 `official` 为前缀，天然合并成一份。
- 测试：精确模式 API 逐账号不合并（两账号同别名各自保留自己的 `context_window`）、官方两账号合并成一份 `official/*`、`[1m]` 孪生带前缀、Codex 每账号窗口。

## 4. 四个调用点接上模式

- `route_proxy_service::build_models_list_payload`（`SelectedCredential` → `CatalogMemberInput`），`/v1/models` 处读 `RoutePoolRepository::model_mode`。
- `route_config_service::resolve_client_models`、`write_codex_model_catalog`。
- `agent_launch_service::platform_models`。
- 测试：`/v1/models` 在两种模式下的 payload；精确模式下 ZCode 配置与 Codex 目录里都是带前缀 id。

## 5. 代理选路

- 抽 `candidate_capability(candidate)`（解析 + 官方剔合成别名），`filter_candidates_for_model` 改用它。
- `resolve_account_scoped_model` + `rewrite_requested_model_value`（递归改写等于原前缀名的 `model`）。
- 在 `forward_request` 里 `filter_candidates_for_rule` 之后调用，替换 `requested_model` 与 `body_bytes`。
- 测试：钉到指定 API 账号；`official/` 只落官方；未知前缀回退；`z-ai/glm-5.3` 两种情形都回退；兜底映射账号不能劫走 `Grox/gpt-5.6-sol`；嵌套 `model` 一并改写；官方账号收到裸名；`前缀/别名[1m]` 保后缀。

## 6. 命令与前端契约

- `set_route_pool_model_mode` 命令 + `lib.rs` invoke_handler + `web/handlers` 派发表。
- `src/lib/api/types.ts` 的 `RoutePoolState` 加 `model_mode`；`client.ts` 加 `setRoutePoolModelMode`。

## 7. 弹窗与接线

- `ConfigWriteTargetsDialog` 加 `modelMode` / `onModelModeChange` props 与分段控件（tab 之上，`radiogroup`），loading 时禁用。
- `AccountsScreen` 传入 `routePoolQuery.data?.model_mode`，切换走新 mutation，成功后 invalidate `route-pool` 与 `route-config-stale`。
- 测试：弹窗回显与回调、AccountsScreen 切换落到命令。

## 8. 文档

`docs-site/docs/guide/protocol-routing.md`、`quick-start.md` 与 `docs-site/docs/en/` 对应两篇补精确模式一节。

## 9. 全量验证

`pnpm typecheck`、`pnpm test:run`、`CARGO_TARGET_DIR=target-codex cargo fmt --check`、`cargo clippy`、`cargo test`。
