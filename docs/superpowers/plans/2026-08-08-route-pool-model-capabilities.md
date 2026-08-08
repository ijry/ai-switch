
# 算力池模型能力匹配与展示实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让算力池在请求前按模型过滤有限能力账号，并让 `/models`、模型映射提示和账号模型标签与实际匹配规则一致。

**Architecture:** 新增 Rust 纯模型能力模块，统一负责请求模型提取、账号映射解析、模型命中和对外模型集合聚合。代理服务使用该模块过滤候选账号和生成 `/models`；React 账号页复用现有 `model_mappings` JSON，增加风险提示和独立的模型别名摘要 popover，不新增数据库字段。

**Tech Stack:** Rust、serde_json、Axum、现有 Tauri service tests、React、TypeScript、Tailwind CSS、Vitest、Testing Library。

## Global Constraints

- 无映射账号视为通配账号，可承接任意请求模型。
- 有映射账号仅在请求模型命中 `from` 时参与模型请求；请求没有 `model` 时保持全池轮询。
- Codex 固定基线模型为 `gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`、`gpt-5.5`。
- `/models` 只有在池中存在通配账号时才加入平台固定基线；始终合并有效映射 `from` 并大小写不敏感去重。
- Claude `supports_1m = true` 时额外公开和匹配对应的 `[1m]` 别名。
- 不新增数据库字段，不改变现有 `model_mappings` JSON 格式，不迁移历史账号。
- 不为 Gemini 增加协议选择下拉框；Gemini API 的模型映射编辑器继续使用通用提示规则。
- `docs/superpowers/specs` 和 `docs/superpowers/plans` 使用中文撰写。
- Rust 验证使用 `CARGO_TARGET_DIR=target-codex`，避免默认 target 锁冲突。

---

### Task 1: 新增统一模型能力模块

**Files:**
- Create: `src-tauri/src/services/route_model_capability.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: `src-tauri/src/services/route_model_capability.rs` 内的 `#[cfg(test)]` 模块

**Interfaces:**
- Produces `ModelCapability { mappings: Vec<ModelMapping> }`。
- Produces `pub(crate) fn requested_model_from_body(body: &[u8]) -> Option<String>`。
- Produces `pub(crate) fn parse_model_capability(config_json: &str) -> ModelCapability`。
- Produces `pub(crate) fn supports_requested_model(capability: &ModelCapability, requested_model: Option<&str>) -> bool`。
- Produces `pub(crate) fn advertised_model_ids(platform: &str, capabilities: &[ModelCapability]) -> Vec<String>`。

- [ ] **Step 1: 写失败的模型能力单测**

在新模块中先写以下测试，测试数据直接使用现有 `ModelMapping` JSON 形状：

~~~rust
#[test]
fn requested_model_reads_only_a_non_empty_top_level_model() {
    assert_eq!(
        requested_model_from_body(br#"{"model":"gpt-5.6-sol","input":"hi"}"#),
        Some("gpt-5.6-sol".to_string())
    );
    assert_eq!(requested_model_from_body(br#"{"nested":{"model":"gpt-5"}}"#), None);
    assert_eq!(requested_model_from_body(br#"{"model":""}"#), None);
    assert_eq!(requested_model_from_body(b"not-json"), None);
}

#[test]
fn empty_mappings_are_wildcard_and_non_empty_mappings_are_restricted() {
    let wildcard = parse_model_capability(r#"{"model_mappings":[]}"#);
    let limited = parse_model_capability(
        r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
    );

    assert!(supports_requested_model(&wildcard, Some("gpt-5.6-luna")));
    assert!(supports_requested_model(&limited, Some("gpt-5.6-sol")));
    assert!(!supports_requested_model(&limited, Some("gpt-5.6-luna")));
    assert!(supports_requested_model(&limited, None));
}
~~~

- [ ] **Step 2: 运行模块窄测并确认失败**

运行：`cd src-tauri; $env:CARGO_TARGET_DIR='target-codex'; cargo test route_model_capability --lib`

预期：FAIL，失败原因是模块、结构体或函数尚未实现。

- [ ] **Step 3: 实现配置解析、匹配和固定基线**

在 `route_model_capability.rs` 中实现：

~~~rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelCapability {
    pub(crate) mappings: Vec<ModelMapping>,
}

pub(crate) fn requested_model_from_body(body: &[u8]) -> Option<String>;
pub(crate) fn parse_model_capability(config_json: &str) -> ModelCapability;
pub(crate) fn supports_requested_model(
    capability: &ModelCapability,
    requested_model: Option<&str>,
) -> bool;
pub(crate) fn advertised_model_ids(
    platform: &str,
    capabilities: &[ModelCapability],
) -> Vec<String>;
~~~

`parse_model_capability` 只接受对象 JSON，读取 `model_mappings` 数组，删除空值和 `upstream-model` 占位映射；无效 JSON 返回空映射能力。固定集合为：

~~~rust
"codex" => ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"],
"claude" => [
    "claude-sonnet-5",
    "claude-opus-4-8",
    "claude-fable-5",
    "claude-haiku-4-5",
],
"gemini" => ["gemini-2.5-flash"],
"grok" => ["grok-4.5"],
~~~

仅当 `capabilities` 中至少一个 `mappings.is_empty()` 时加入固定集合；之后按输入顺序追加映射 `from`，使用小写 key 去重。Claude 映射带 `supports_1m = Some(true)` 时追加 `{from}[1m]`，已经带 `[1m]` 的别名不得重复追加。模型命中复用现有 Claude `[1m]` 归一化规则，普通平台使用 trim 后的精确匹配。

- [ ] **Step 4: 运行模块窄测并确认通过**

运行：`cd src-tauri; $env:CARGO_TARGET_DIR='target-codex'; cargo test route_model_capability --lib`

预期：PASS，并补充断言 Codex 基线、有限映射不返回未映射基线、大小写去重和 Claude 1M 展开。

- [ ] **Step 5: 提交模型能力模块**

~~~powershell
git add src-tauri/src/services/mod.rs src-tauri/src/services/route_model_capability.rs
git commit -m "feat: add route model capability rules"
~~~

### Task 2: 接入代理候选过滤和 `/models`

**Files:**
- Modify: `src-tauri/src/services/route_proxy_service.rs`
- Test: `src-tauri/src/services/route_proxy_service.rs` 内的代理单测

**Interfaces:**
- Consumes `route_model_capability::requested_model_from_body`、`parse_model_capability`、`supports_requested_model` 和 `advertised_model_ids`。
- Produces `fn filter_credentials_for_model(credentials: Vec<SelectedCredential>, requested_model: Option<&str>) -> Vec<SelectedCredential>`。
- Produces稳定错误文本前缀 `route_pool.model_unmatched`，供现有 `json_error` 选择错误码。

- [ ] **Step 1: 写候选过滤和模型列表失败测试**

在代理测试模块中增加：

~~~rust
#[test]
fn filter_credentials_for_model_keeps_wildcard_and_matching_mappings_only() {
    let wildcard = api_credential_with_config("wildcard", r#"{"model_mappings":[]}"#);
    let sol = api_credential_with_config(
        "sol",
        r#"{"model_mappings":[{"from":"gpt-5.6-sol","to":"sol-upstream"}]}"#,
    );
    let luna = api_credential_with_config(
        "luna",
        r#"{"model_mappings":[{"from":"gpt-5.6-luna","to":"luna-upstream"}]}"#,
    );

    let selected = filter_credentials_for_model(
        vec![wildcard.clone(), sol.clone(), luna],
        Some("gpt-5.6-sol"),
    );

    assert_eq!(
        selected.iter().map(|item| item.display_name.as_str()).collect::<Vec<_>>(),
        vec!["wildcard", "sol"]
    );
}
~~~

同时将现有 `/models` 测试扩展为：有通配账号时返回四个 Codex 基线加映射别名；全是有限映射时不返回未映射的 `gpt-5.6-luna`。

- [ ] **Step 2: 运行代理窄测并确认失败**

运行：`cd src-tauri; $env:CARGO_TARGET_DIR='target-codex'; cargo test services::route_proxy_service::tests --lib`

预期：FAIL，失败原因是过滤函数不存在或旧 `/models` 结果仍只聚合映射。

- [ ] **Step 3: 在普通请求路径接入模型过滤**

调整 `forward_request` 的顺序：保留 `/models` GET 快路径；普通请求在查询池账号前读取 body，调用 `requested_model_from_body`，再执行现有 `filter_credentials_for_rule`，最后调用 `filter_credentials_for_model`。当请求模型非空且过滤结果为空，返回包含以下前缀的错误：

~~~text
route_pool.model_unmatched: no enabled route credential supports model '<model>' on platform '<platform>'
~~~

请求体无效、没有模型字段或模型为空时不调用限制过滤，保证无模型请求仍能沿用全池轮询。

- [ ] **Step 4: 复用统一能力模块生成 `/models`**

删除代理文件中重复的 `model_mappings`、占位过滤、`collect_pool_model_ids` 和重复去重实现。 `json_models_list_response` 为每个候选账号调用 `parse_model_capability`，把能力数组传给 `advertised_model_ids`。更新 `json_error`，识别 `route_pool.model_unmatched` 并返回稳定 `code`，不把账号密钥写入错误详情。

- [ ] **Step 5: 运行代理窄测并确认通过**

运行：`cd src-tauri; $env:CARGO_TARGET_DIR='target-codex'; cargo test services::route_proxy_service::tests --lib`

预期：PASS；现有协议桥接、重试、模型列表和请求构造测试不回归。

- [ ] **Step 6: 提交代理接入**

~~~powershell
git add src-tauri/src/services/route_proxy_service.rs
git commit -m "feat: route pool requests by model capability"
~~~

### Task 3: 增加模型映射提示

**Files:**
- Modify: `src/screens/AccountsScreen.tsx`
- Test: `tests/AccountsScreen.test.tsx`

**Interfaces:**
- Consumes 现有 `ModelMappingsEditor` 的 `value`、`fetchedModels` 和 `platform`。
- Produces UI 文案：无映射时提示有限模型风险；有映射时提示当前别名数量。

- [ ] **Step 1: 写前端失败测试**

在 API 账号创建测试中打开账号弹窗，断言默认空映射时出现风险提示；在已有映射编辑测试中断言出现已配置数量提示：

~~~tsx
expect(
  screen.getByText(/上游只支持有限模型.*建议.*配置模型映射/),
).toBeInTheDocument();

expect(screen.getByText(/当前账号仅按已配置的本地模型别名参与匹配/)).toBeInTheDocument();
~~~

- [ ] **Step 2: 运行前端窄测并确认失败**

运行：`pnpm test:run -- tests/AccountsScreen.test.tsx`

预期：FAIL，因为 `ModelMappingsEditor` 尚未渲染新文案。

- [ ] **Step 3: 实现编辑器提示**

在现有“留空表示不改写模型”说明之前插入提示块：

~~~tsx
{value.length === 0 ? (
  <p className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] font-semibold leading-5 text-amber-900">
    如果上游只支持有限模型，建议先获取模型列表并配置模型映射；配置后算力池只会把该账号匹配到映射别名。
  </p>
) : (
  <p className="rounded-lg border border-blue-100 bg-blue-50 px-3 py-2 text-[11px] font-medium leading-5 text-blue-900">
    当前账号仅按已配置的本地模型别名参与匹配，共 {value.length} 条。
  </p>
)}
~~~

提示不改变保存校验，空映射仍允许提交；不修改协议选择下拉。

- [ ] **Step 4: 运行前端窄测并确认通过**

运行：`pnpm test:run -- tests/AccountsScreen.test.tsx`

预期：PASS，已有账号创建、编辑、模型获取和映射保存测试继续通过。

- [ ] **Step 5: 提交编辑器提示**

~~~powershell
git add src/screens/AccountsScreen.tsx tests/AccountsScreen.test.tsx
git commit -m "feat: explain route model mapping requirements"
~~~

### Task 4: 实现账号模型标签和完整映射 popover

**Files:**
- Create: `src/components/accounts/ModelMappingSummary.tsx`
- Create: `tests/ModelMappingSummary.test.tsx`
- Modify: `src/screens/AccountsScreen.tsx`

**Interfaces:**
- Produces `export type DisplayModelMapping = { alias: string; target: string; label?: string | null; oneM: boolean }`。
- Produces `export function expandDisplayModelMappings(platform: string, mappings: ModelMapping[]): DisplayModelMapping[]`。
- Produces `export function ModelMappingSummary(props: { platform: string; mappings: ModelMapping[] }): JSX.Element`。

- [ ] **Step 1: 写组件失败测试**

在 `tests/ModelMappingSummary.test.tsx` 中覆盖空、三条和超过三条映射：

~~~tsx
it("shows wildcard state for empty mappings", () => {
  render(<ModelMappingSummary platform="codex" mappings={[]} />);
  expect(screen.getByText("模型通配")).toBeInTheDocument();
});

it("shows three aliases and opens the remaining mappings", async () => {
  const user = userEvent.setup();
  render(
    <ModelMappingSummary
      platform="codex"
      mappings={[
        { from: "a", to: "up-a" },
        { from: "b", to: "up-b" },
        { from: "c", to: "up-c" },
        { from: "d", to: "up-d" },
      ]}
    />,
  );

  expect(screen.getByText("a")).toBeInTheDocument();
  expect(screen.getByText("b")).toBeInTheDocument();
  expect(screen.getByText("c")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "+1" }));
  expect(screen.getByText("d → up-d")).toBeInTheDocument();
});
~~~

- [ ] **Step 2: 运行组件窄测并确认失败**

运行：`pnpm test:run -- tests/ModelMappingSummary.test.tsx`

预期：FAIL，因为组件和展开函数尚未创建。

- [ ] **Step 3: 实现别名展开函数**

`expandDisplayModelMappings` 按输入顺序转换映射；对 Claude 且 `supports_1m === true` 的条目追加一个 `oneM: true`、alias 为 `{from}[1m]` 的显示条目，已有 `[1m]` 后缀时不重复追加。其他平台只保留原映射。

- [ ] **Step 4: 实现 popover 组件**

组件行为固定为：

1. `mappings.length === 0` 渲染 `模型通配` 标签。
2. 有映射时渲染前三个 alias 标签；超过三条渲染一个 `+N` 按钮。
3. `+N` 使用 `aria-expanded` 和 `aria-haspopup="dialog"`，点击后显示完整条目列表。
4. 完整条目显示 `alias → target`，有 `label` 时追加 label，有 `oneM` 时追加 `[1m]`。
5. 外层 `relative`，弹层 `absolute`，设置最大宽度和滚动高度；alias/target 通过 `title` 保留完整值。
6. 使用 `useRef` 监听外部 pointerdown，使用 `useEffect` 监听 Escape；关闭时清理监听器。

- [ ] **Step 5: 接入账号行**

在 `AccountsScreen.tsx` 的账号行中调用：

~~~tsx
<ModelMappingSummary
  platform={activePlatform}
  mappings={parseModelMappingsFromConfig(credential.config_json)}
/>
~~~

放在账号状态 tag 区域之后或同一行的可换行区域内，不改变拖拽按钮、复选框和右侧操作按钮位置。

- [ ] **Step 6: 运行组件和账号页测试**

运行：`pnpm test:run -- tests/ModelMappingSummary.test.tsx tests/AccountsScreen.test.tsx`

预期：PASS，确认模型标签不会影响现有账号创建、编辑、筛选、拖拽和批量操作测试。

- [ ] **Step 7: 提交账号模型展示**

~~~powershell
git add src/components/accounts/ModelMappingSummary.tsx tests/ModelMappingSummary.test.tsx src/screens/AccountsScreen.tsx
git commit -m "feat: show route account model mappings"
~~~

### Task 5: 全量验证和交付检查

**Files:**
- Modify: 无新增业务文件；只检查前述任务产物和工作区状态。
- Test: 前端、Rust、Go 和 CI 对应命令。

**Interfaces:**
- Consumes 前述 Rust 模型能力 API、代理过滤路径和 React 模型摘要组件。
- Produces 可推送的干净工作区和可复现的验证结果。

- [ ] **Step 1: 运行类型检查和前端全量测试**

运行：

~~~powershell
pnpm typecheck
pnpm test:run
~~~

预期：TypeScript 无错误，Vitest 全部通过。

- [ ] **Step 2: 运行 Rust 全量检查和测试**

运行：

~~~powershell
$env:CARGO_TARGET_DIR='target-codex'; pnpm rust:check
$env:CARGO_TARGET_DIR='target-codex'; pnpm rust:test
~~~

预期：Rust 编译和全部单测通过；允许记录既有 warning，但不得新增失败。

- [ ] **Step 3: 运行仓库格式和旧协议扫描**

运行：

~~~powershell
git diff --check
rg -n "anthropic-messages|anthropic_messages" src-tauri/src src tests
~~~

预期：差异无空白错误；旧协议扫描不命中业务代码。

- [ ] **Step 4: 审核提交内容**

运行：

~~~powershell
git status --short
git diff HEAD~4..HEAD --stat
git log -5 --oneline --decorate
~~~

确认没有 `target-codex`、`dist`、密钥或测试生成物进入提交。

- [ ] **Step 5: 提交最终验证结果**

运行：

~~~powershell
git status --short
~~~

预期：工作区干净；如果用户要求推送，则执行 `git push origin main` 并用 `git ls-remote origin refs/heads/main` 核对远端哈希。
