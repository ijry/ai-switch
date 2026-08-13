# Skills 与技能包统一入口实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 `Skills` 页面内增加“技能/技能包”两个子 Tab，提供 AI Switch 内置核心和科学技能包，并让包级信息与单 Skill 管理共享元数据。

**Architecture:** 继续以磁盘上的 Skill 文件作为单 Skill 权威来源，以 AI Switch 内置 catalog 作为技能包权威来源。后端新增独立的包聚合和安装模块，复用现有 Skill 路径、front matter 和读取流程；技能包资源随应用分发，不读取或扫描 Codeg。前端在 `SkillsScreen` 内增加视图状态和包面板，单 Skill 保存/删除命令保持原样。

**Tech Stack:** Rust 2021、`serde`、`serde_json`、`serde_yaml`、React 18、TypeScript、TanStack Query、Tailwind CSS、Vitest、Testing Library。

## Global Constraints

- 顶层导航只保留现有 `Skills`，不新增“技能包”顶层路由。
- `SkillsScreen` 内部 Tab 固定为 `skills` 和 `packages`，默认显示 `skills`。
- 第一阶段使用 AI Switch 内置 `ai-switch.core` 和 `ai-switch.science` catalog，不联网、不搜索、不更新、不卸载技能包。
- 技能包 ID 固定为 `ai-switch.core` 和 `ai-switch.science`。
- Codeg 不作为 AI Switch 的 Skill 扫描来源；Codeg 中已安装的 Skill 不影响 AI Switch 的安装状态。
- 包面板提供查看、刷新、安装缺失技能和成员跳转，不显示不会影响 Agent 实际行为的启用/禁用开关。
- 单 Skill 的 `skills_save`、`skills_delete` 命令和后端只读保护保持不变。
- 不覆盖工作区中已有更新检查改动、旧迁移设计和旧迁移计划。
- 本计划只生成实施步骤；执行阶段未获用户明确要求时不执行 `git commit`。

---

### Task 1: 扩展 Skill 和技能包数据契约

**Files:**
- Modify: `src-tauri/src/skills/model.rs`
- Modify: `src-tauri/src/skills/frontmatter.rs`
- Modify: `src/lib/api/types.ts`
- Modify: `src/components/skills/catalog.ts`
- Test: `src-tauri/src/skills/frontmatter.rs`
- Test: `src-tauri/src/skills/model.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Builtin,
    Codex,
    Agents,
    Project,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillPackage {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source: SkillSource,
    pub version: Option<String>,
    pub manifest_path: Option<String>,
    pub skill_ids: Vec<String>,
    pub skill_count: usize,
    pub installed_skill_ids: Vec<String>,
    pub installed_count: usize,
    pub installed_at: Option<String>,
    pub read_only: bool,
    pub target_clients: Vec<SkillAgentType>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillScanWarning {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillsPackageListResult {
    pub packages: Vec<SkillPackage>,
    pub skills: Vec<SkillItem>,
    pub warnings: Vec<SkillScanWarning>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillPackageDetail {
    pub package: SkillPackage,
    pub skills: Vec<SkillItem>,
}
```

`SkillItem` 增加：

```rust
pub package_id: Option<String>,
pub package_name: Option<String>,
pub category: Option<String>,
pub tags: Vec<String>,
pub language: Option<String>,
pub source: SkillSource,
pub version: Option<String>,
pub installed_at: Option<String>,
pub target_clients: Vec<SkillAgentType>,
```

在 TypeScript 中同步定义 `SkillSource`、`SkillPackage`、`SkillScanWarning`、`SkillsPackageListResult` 和 `SkillPackageDetail`，字段使用现有 API 的 snake_case 命名。

- [ ] **Step 1: 写 front matter 扩展测试**

```rust
#[test]
fn parses_optional_skill_metadata_without_breaking_old_files() {
    let value = parse_skill_metadata(
        "---\nname: demo\ncategory: tools\ntags: [filesystem, io]\nlanguage: en\ndisplay_name: Demo\n---\n# Body",
    )
    .unwrap()
    .unwrap();
    assert_eq!(value.category.as_deref(), Some("tools"));
    assert_eq!(value.tags, vec!["filesystem", "io"]);
    assert_eq!(value.language.as_deref(), Some("en"));
    assert_eq!(value.display_name.as_deref(), Some("Demo"));
}

#[test]
fn missing_optional_metadata_uses_empty_values() {
    let value = parse_skill_metadata("---\nname: demo\n---\n# Body").unwrap().unwrap();
    assert!(value.category.is_none());
    assert!(value.tags.is_empty());
    assert!(value.language.is_none());
}
```

- [ ] **Step 2: 运行 Rust 测试确认失败**

运行：`cd src-tauri && cargo test skills::frontmatter::tests`

预期：FAIL，因为 `SkillMetadata` 目前只有 `name` 和 `description`。

- [ ] **Step 3: 扩展 front matter 类型**

在 `SkillMetadata` 中增加 `category`、`tags`、`language` 和 `display_name`。`tags` 支持 YAML 字符串数组；为兼容手写 front matter，同时接受逗号分隔字符串并标准化为去空白、去重后的 `Vec<String>`。缺少字段时返回空数组或 `None`，不阻塞旧 Skill。

- [ ] **Step 4: 扩展 Rust/TypeScript 模型**

为 Rust 模型增加 `SkillSource`、`SkillPackage`、`SkillScanWarning` 和 `SkillsPackageListResult`，在 TypeScript `src/lib/api/types.ts` 中使用 snake_case 字段保持序列化一致。`SkillItem` 的新增字段全部使用可空或空数组回退，避免现有 fixture 必须一次性补齐所有元数据。

在 `src/components/skills/catalog.ts` 中建立包名和常见 Skill 的 i18n key 映射，只保存稳定 ID，不把中文文本硬编码到后端扫描结果。

- [ ] **Step 5: 运行测试确认通过**

运行：`cd src-tauri && cargo test skills::frontmatter::tests skills::model::tests && pnpm typecheck`

预期：PASS；旧 Skill fixture 和新元数据 fixture 都能反序列化。

### Task 2: 固定 AI Switch Skill 扫描来源

**Files:**
- Modify: `src-tauri/src/skills/paths.rs`
- Modify: `src-tauri/src/skills/service.rs`
- Test: `src-tauri/src/skills/paths.rs`
- Test: `src-tauri/src/skills/service.rs`

**Interfaces:**
- `skill_storage_spec(SkillAgentType::Codex)` 只返回 AI Switch 管理的 Codex、`.system` 和 Agents 目录。
- `SkillStorageSpec::source_for_path(path: &Path) -> SkillSource` 只识别现有 AI Switch 扫描来源。
- 现有 `list_skills(...)`、`read_skill(...)`、`save_skill(...)`、`delete_skill(...)` 函数签名不变。

- [ ] **Step 1: 写 Codeg 排除测试**

```rust
#[test]
fn codex_does_not_scan_codeg_skills() {
    let spec = skill_storage_spec(SkillAgentType::Codex);
    let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
    let codeg_root = home.join(".codeg/skills");
    assert!(!spec.global_dirs.contains(&codeg_root));
    assert!(!spec.read_only_roots.contains(&codeg_root));
}
```

- [ ] **Step 2: 运行测试确认失败**

运行：`cd src-tauri && cargo test skills::paths::tests::codex_does_not_scan_codeg_skills`

预期：FAIL，因为旧实现仍会把 `.codeg/skills` 加入 Codex 扫描目录。

- [ ] **Step 3: 移除 Codeg global root 并保留来源识别**

从 Codex 的 `global_dirs` 和 `read_only_roots` 中移除 Codeg 路径，并保留来源判断：

```text
~/.codex/skills/.system  -> builtin
~/.agents/skills         -> agents
其他 Codex 全局目录       -> codex
项目相对目录               -> project
```

判断必须使用 canonicalized path 前缀，不能依赖字符串包含。
路径测试直接使用 `directories::BaseDirs` 得到当前测试用户 home，不引入新的环境变量覆盖机制。

- [ ] **Step 4: 将来源和 front matter 元数据写入 SkillItem**

修改 `list_skills_from_dir` 接收 `SkillSource`、包元数据索引和目标客户端列表；构造 `SkillItem` 时填充：

1. `display_name` 优先于 `name`。
2. `category`、`tags`、`language` 来自 front matter。
3. 技能包信息由 AI Switch 内置 catalog 合并，不扫描外部 manifest。
4. `read_only` 由目录规格判断，不能由前端决定。

保存和删除继续遵守 `.system` 等 AI Switch 内置只读目录的后端保护。

- [ ] **Step 5: 运行测试确认通过**

运行：`cd src-tauri && cargo test skills::paths::tests skills::service::tests`

预期：PASS；现有 Codex/.system/Agents 行为不回归，Codeg 目录不出现在扫描结果中。

### Task 3: 实现 AI Switch catalog 和技能包聚合

**Files:**
- Create: `src-tauri/src/skills/packages.rs`
- Modify: `src-tauri/src/skills/mod.rs`
- Modify: `src-tauri/src/skills/service.rs`
- Test: `src-tauri/src/skills/packages.rs`

**Interfaces:**

```rust
pub fn list_skill_packages(
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillsPackageListResult, AppError>;

pub fn read_skill_package(
    package_id: &str,
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillPackageDetail, AppError>;

pub fn install_skill_package(
    package_id: &str,
    agent: Option<SkillAgentType>,
    scope: Option<SkillScope>,
    workspace_path: Option<&Path>,
) -> Result<SkillPackageInstallResult, AppError>;
```

实现细节：

- `ai-switch.core` 和 `ai-switch.science` 由 AI Switch 内置 Rust catalog 定义。
- 包成员安装状态只看当前 Agent/Scope 管理目录扫描结果里是否存在同 ID Skill。
- 安装包时从 `src-tauri/resources/skill-packages/<package-id>/<skill-id>` 复制缺失技能。
- 若同 ID 已存在，安装时跳过，不覆盖已有文件。
- 第一阶段不做整包卸载，因为无法证明已有同 ID Skill 的所有权。

- [ ] **Step 1: 写 catalog fixture 测试**

```rust
#[test]
fn exposes_ai_switch_owned_packages() {
    let result = package_result_from_installed(Vec::new());
    assert!(result.packages.iter().any(|item| item.id == "ai-switch.core"));
    assert!(result.packages.iter().any(|item| item.id == "ai-switch.science"));
}

#[test]
fn marks_same_skill_id_as_installed_without_source_distinction() {
    let mut skill = skill("brainstorming");
    skill.source = SkillSource::Agents;
    let result = package_result_from_installed(vec![skill]);
    let core = result.packages.iter().find(|item| item.id == "ai-switch.core").unwrap();
    assert_eq!(core.installed_skill_ids, vec!["brainstorming"]);
}
```

- [ ] **Step 2: 运行测试确认失败**

运行：`cd src-tauri && cargo test skills::packages::tests`

预期：FAIL，因为包扫描模块尚未实现 AI Switch catalog。

- [ ] **Step 3: 实现 catalog 和安装状态聚合**

在 `packages.rs` 中定义固定包 ID 和成员 Skill ID。`list_skill_packages` 调用 `list_skills` 得到当前筛选上下文的 Skill 列表，按同 ID 填充 `installed_skill_ids`、`installed_count` 和成员状态。

- [ ] **Step 4: 实现安装缺失技能**

将技能资源移植到 `src-tauri/resources/skill-packages`，并在 `tauri.conf.json` 中打包为 `skill-packages/`。安装命令只复制缺失 ID 到当前 Agent/Scope 的第一个可写目录；同 ID 已安装时放入 `skipped_skill_ids`。

- [ ] **Step 5: 运行测试确认通过**

运行：`cd src-tauri && cargo test skills::packages::tests`

预期：PASS，覆盖 AI Switch 包、同 ID 安装检测、安装跳过不覆盖和未知包。

### Task 4: 暴露技能包只读 API

**Files:**
- Modify: `src-tauri/src/skills/command.rs`
- Modify: `src-tauri/src/skills/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/web/handlers/mod.rs`
- Modify: `src/lib/api/client.ts`
- Modify: `src/lib/api/types.ts`
- Modify: `tests/transport/command-contract.test.ts`

**Interfaces:**

```text
skills_list_packages(agent_type?, scope?, workspace_path?)
skills_read_package(package_id, agent_type?, scope?, workspace_path?)
skills_install_package(package_id, agent_type?, scope?, workspace_path?)
```

- [ ] **Step 1: 先增加 TypeScript 类型和 API 封装**

在 `src/lib/api/client.ts` 增加：

```ts
export function skillsListPackages(input?: {
  agentType?: SkillAgentType;
  scope?: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillsPackageListResult> {
  return invoke("skills_list_packages", {
    agentType: input?.agentType ?? null,
    scope: input?.scope ?? null,
    workspacePath: input?.workspacePath ?? null,
  });
}

export function skillsReadPackage(input: {
  packageId: string;
  agentType?: SkillAgentType;
  scope?: SkillScope;
  workspacePath?: string | null;
}): Promise<SkillPackageDetail> {
  return invoke("skills_read_package", {
    packageId: input.packageId,
    agentType: input.agentType ?? null,
    scope: input.scope ?? null,
    workspacePath: input.workspacePath ?? null,
  });
}
```

现有 `client.ts` 的 invoke wrapper 会将 camelCase 转换为后端命令需要的字段；若当前 wrapper 不转换，统一改为显式 snake_case，不在两个命令中混用。

- [ ] **Step 2: 写命令契约失败测试**

在 `tests/transport/command-contract.test.ts` 增加断言：

```ts
expect(registeredCommands).toContain("skills_list_packages");
expect(registeredCommands).toContain("skills_read_package");
expect(sensitiveCommands).not.toContain("skills_list_packages");
expect(sensitiveCommands).not.toContain("skills_read_package");
```

- [ ] **Step 3: 运行契约测试确认失败**

运行：`pnpm test:run -- tests/transport/command-contract.test.ts`

预期：FAIL，因为 Rust command 和 Web dispatch 尚未注册。

- [ ] **Step 4: 实现 Tauri command**

在 `src-tauri/src/skills/command.rs` 增加两个只读 command 和一个敏感写入安装 command，参数使用 `Option<SkillAgentType>`、`Option<SkillScope>` 和 `Option<String>`，只调用 `service::list_skill_packages`/`service::read_skill_package`，不在 command 层读取文件。

将两个函数加入 `generate_handler!`，并从 `skills/mod.rs` 导出。

- [ ] **Step 5: 实现 Web dispatch**

在 `src-tauri/src/web/handlers/mod.rs` 增加两个只读分支。`skills_list_packages` 和 `skills_read_package` 不加入敏感写命令集合，`skills_install_package` 必须加入敏感写命令集合；仍需遵守现有 Web transport 的认证和响应封装。

- [ ] **Step 6: 运行契约和类型检查**

运行：

```text
pnpm test:run -- tests/transport/command-contract.test.ts
pnpm typecheck
cd src-tauri && cargo test web::handlers::tests
```

预期：PASS；两个只读命令在 Tauri/Web 可调用，现有五个 Skill 命令行为不变。

### Task 5: 在 Skills 页面加入双 Tab 和技能包面板

**Files:**
- Create: `src/components/skills/SkillsTabs.tsx`
- Create: `src/components/skills/SkillPackagesList.tsx`
- Create: `src/components/skills/SkillPackageDetail.tsx`
- Modify: `src/screens/SkillsScreen.tsx`
- Modify: `src/components/skills/SkillsList.tsx`
- Modify: `src/lib/i18n.tsx`
- Modify: `tests/SkillsScreen.test.tsx`
- Create: `tests/SkillPackages.test.tsx`

**Interfaces:**

```tsx
type SkillsView = "skills" | "packages";

type SkillsTabsProps = {
  value: SkillsView;
  onChange: (value: SkillsView) => void;
};

type SkillPackagesListProps = {
  packages: SkillPackage[];
  selectedId: string | null;
  onSelect: (packageId: string) => void;
};

type SkillPackageDetailProps = {
  detail: SkillPackageDetail | null;
  loading: boolean;
  onSelectSkill: (skill: SkillItem) => void;
};
```

- [ ] **Step 1: 写双 Tab 和包面板失败测试**

```tsx
it("keeps Skills as the top-level screen and exposes two internal tabs", async () => {
  render(<SkillsScreen />);
  expect(screen.getByRole("tab", { name: "技能" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "技能包" })).toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "技能包" })).not.toBeInTheDocument();
});

it("shows AI Switch core and science packages", async () => {
  vi.mocked(skillsListPackages).mockResolvedValue(packageFixture);
  render(<SkillsScreen />);
  await userEvent.click(screen.getByRole("tab", { name: "技能包" }));
  expect(await screen.findByText("AI Switch 核心技能包")).toBeInTheDocument();
  expect(screen.getByText("AI Switch 科学技能包")).toBeInTheDocument();
});
```

- [ ] **Step 2: 运行测试确认失败**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx tests/SkillPackages.test.tsx`

预期：FAIL，因为 Skills 页面当前没有内部 Tab、包查询或包组件。

- [ ] **Step 3: 实现内部 Tab**

在 `SkillsScreen` 中增加：

```ts
const [view, setView] = useState<SkillsView>("skills");
```

在现有 `SkillsToolbar` 之后渲染 `SkillsTabs`。Tab 使用 `role="tablist"`、`role="tab"`、`aria-selected`；不新增路由，不改变 `AppLayout` 的顶层导航。

- [ ] **Step 4: 实现技能包查询和列表**

增加 React Query：

```ts
const packagesQuery = useQuery({
  queryKey: ["skills-packages", agentType, scope, workspacePath],
  queryFn: () => skillsListPackages({
    agentType,
    scope,
    workspacePath: scope === "project" ? workspacePath : null,
  }),
  enabled: view === "packages" && (scope === "global" || Boolean(workspacePath.trim())),
});
```

`SkillPackagesList` 显示包名、来源、版本、安装时间、Skill 数量和 warning；包列表为空时显示“未发现本机技能包”，manifest warning 单独显示。

- [ ] **Step 5: 实现包详情和成员跳转**

选中包后调用 `skillsReadPackage`，`SkillPackageDetail` 显示包级元数据和成员 Skill 列表。成员点击执行：

```ts
setView("skills");
setSelectedId(skill.id);
setCreating(false);
setEditing(false);
```

成员跳转后复用既有 `skillsRead`。若成员来自 `.system`，`read_only` 保持为 true，因此编辑/删除按钮不会被错误开放。

- [ ] **Step 6: 运行前端测试确认通过**

运行：`pnpm test:run -- tests/SkillsScreen.test.tsx tests/SkillPackages.test.tsx`

预期：PASS；默认进入“技能”，切换到“技能包”可看到包列表、详情、warning 和成员跳转，现有单 Skill 编辑流程不回归。

### Task 6: 完成统一入口回归验证

**Files:**
- Modify: `tests/SkillsScreen.test.tsx`
- Modify: `tests/SkillPackages.test.tsx`
- Modify: `src-tauri/src/skills/*`（仅修复本计划测试发现的问题）

- [ ] **Step 1: 运行 Skills 相关前端测试**

运行：

```text
pnpm typecheck
pnpm test:run -- tests/SkillsScreen.test.tsx tests/SkillPackages.test.tsx tests/transport/command-contract.test.ts
```

预期：PASS；两个子 Tab、包扫描、成员跳转、双语文案和 API 契约全部通过。

- [ ] **Step 2: 运行 Rust Skills 测试**

运行：`cd src-tauri && cargo test skills::`

预期：PASS；front matter、路径来源、同 ID 安装检测、包聚合、只读和 command 测试全部通过。

- [ ] **Step 3: 运行全量回归**

运行：

```text
pnpm test:run
pnpm rust:check
pnpm rust:test
```

预期：既有 MCP、更新检查、设置和 transport 测试不回归。

- [ ] **Step 4: 执行手工验收**

使用本机现有 Skill 目录验证：

1. “技能包”能显示 AI Switch 核心包和科学包。
2. 已在 AI Switch 管理的 Codex 或 Agents 目录存在的同 ID Skill 显示为已安装；Codeg 目录中的同 ID Skill 不影响状态。
3. 点击“安装缺失技能”只复制缺失 ID，不覆盖已有同 ID Skill。
4. `.system` Skill 可读取但不能保存/删除。
5. 从包成员跳转到“技能”后，左侧选中项与右侧内容一致。
6. 375px、768px、1024px、1440px 下无页面级横向溢出。

