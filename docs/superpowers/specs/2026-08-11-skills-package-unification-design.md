# Skills 与技能包统一入口设计

## 背景

AI Switch 当前已经具备 MCP 和 Skills 设置页面，但 Skills 页面仍主要按单个 Skill 目录管理，尚未承载 Codeg“技能包”这一层用户概念。用户从设置入口看到的是两个相互分离的能力，无法先按技能包了解来源、分类和安装状态，再进入包内查看具体技能。

本次改造不再新增独立的“技能包”顶层导航，而是在现有 `Skills` 页面内部合并两种用户操作：

```text
MCP
Skills
  ├─ 技能
  └─ 技能包
```

同时修复 MCP/Skills 页面硬编码英文、API 错误直接显示英文，以及 Skills 左侧列表撑破布局的问题。

本规格针对当前仓库已有的 MCP/Skills 能力进行后续改造，不替代 `2026-08-11-mcp-skills-settings-migration-design.md` 中的初始移植设计。

## 目标

- 顶层只保留 `MCP` 和 `Skills`，不增加独立的技能包导航。
- 在 `Skills` 页面内提供“技能”和“技能包”两个子 Tab。
- 让单个 Skill 与所属技能包共享来源、分类、标签、语言、版本和安装状态等展示元数据。
- 由 AI Switch 内置核心技能包和科学技能包 catalog，并展示其包含的 Skill 与安装进度。
- 保持单个 Skill 现有的列表、读取、编辑、保存和删除流程。
- 第一阶段完成 MCP/Skills 全量中英文界面和错误提示本地化。
- 修复 Skills 双栏布局，使左侧列表和右侧内容各自滚动且不产生横向溢出。
- 不联网、不增加远程技能包搜索；不伪造不会影响 Agent 实际行为的启用/禁用状态。

## 非目标

- 不在本次工作中实现远程搜索、远程安装、远程更新或市场功能。
- 不依赖 Codeg manifest 作为技能包权威来源。
- 不把 Skill 文件或技能包 catalog 复制进数据库作为运行时唯一数据源。
- 不将 MCP 配置和 Skills/技能包合并成同一数据模型。
- 不在本期默认提供整包删除、卸载或批量覆盖操作。
- 不重构与本需求无关的账户、模型、路由和设置页面。

## 分阶段交付

### Phase 1：本地化与布局修复

1. 为 `McpScreen`、`McpAppSelector`、`SkillsScreen`、`SkillsToolbar`、`SkillsList` 和 Skill 编辑区域补齐 `en`/`zh-CN` 文案键。
2. 前端根据现有结构化错误码显示本地化错误；未知错误使用通用回退文案。
3. 修复 Skills 主网格、左侧列表、右侧预览/编辑器的 `min-w-0`、`min-h-0`、flex 和 overflow 约束。
4. 修复 `SkillsToolbar` 大屏网格列数与实际控件数量不一致的问题。
5. 添加 MCP/Skills 的双语渲染和错误映射测试。

### Phase 2：Skills 与技能包统一入口

1. 保留现有 `Skills` 顶层入口，在页面内部增加子 Tab。
2. 保留现有单 Skill 数据模型和编辑流程，在其上增加可选包元数据。
3. 使用 AI Switch 内置技能包 catalog 构造技能包列表，并用扫描到的同 ID Skill 判断安装状态。
4. 新增技能包只读浏览界面，不改变 Codeg 的文件安装语义。
5. 通过共享元数据和统一筛选能力，让“技能”和“技能包”从用户角度形成一套管理入口。

## 方案选择

采用“统一页面、内部双 Tab、共享元数据、独立数据模型”的方案。

### 方案 A：统一页面内部双 Tab（采用）

- 顶层只暴露 `Skills`。
- `技能` 继续管理单个 Skill 文件。
- `技能包` 管理包级信息和包内成员浏览。
- 两类数据通过 `package_id`、`source` 等字段关联，但不强行共用保存/删除接口。

优点是用户入口统一，现有单 Skill 编辑能力可以渐进保留，技能包不会被错误地当成一个可编辑文件。实现成本和数据迁移风险也低于重做整个 Skills 后端。

### 方案 B：所有技能扁平化为一个列表

只保留一个列表，通过分类和来源字段区分技能包。

该方案实现简单，但会丢失包级视图，用户无法快速理解某个包包含哪些技能，也不符合 Codeg 的技能包操作习惯。

### 方案 C：技能包完全替代单 Skill 管理

只允许通过包安装器管理 Skill，单个 Skill 不再编辑。

该方案破坏当前已有的单 Skill 新建、编辑和删除能力，也无法覆盖项目级自定义 Skill，因此不采用。

## 统一数据模型

### Skill 元数据

现有 `SkillItem` 保留原字段，并增加可选元数据：

```text
SkillItem
  id: string
  name: string
  scope: global | project
  layout: markdown_file | skill_directory
  path: string
  description: string | null
  read_only: boolean
  package_id: string | null
  package_name: string | null
  category: string | null
  tags: string[]
  language: string | null
  source: builtin | codex | agents | project | unknown
  version: string | null
  installed_at: string | null
  target_clients: SkillAgentType[]
```

字段规则：

- `package_id` 为空时，Skill 仍可作为独立用户/项目 Skill 展示。
- `category`、`tags`、`language` 不是必填，缺少时不阻塞扫描。
- `version` 和 `installed_at` 仅在来源实际提供时返回；AI Switch 内置技能包第一阶段返回 `null`。
- `target_clients` 表示扫描到该 Skill 的客户端，不代表 AI Switch 可以替 Agent 修改其运行时开关。
- 文件路径、Skill ID、版本和哈希等技术值不翻译。

### SkillPackage 模型

新增包级只读模型：

```text
SkillPackage
  id: string
  name: string
  description: string | null
  source: builtin | codex | agents | project | unknown
  version: string | null
  manifest_path: string | null
  skill_ids: string[]
  skill_count: number
  installed_skill_ids: string[]
  installed_count: number
  installed_at: string | null
  read_only: boolean
  target_clients: SkillAgentType[]
```

`SkillPackage` 只负责包级展示和成员聚合；第一期不提供包级保存、删除或启用/禁用命令。

## 本地来源与包识别

### AI Switch 内置技能包 catalog

AI Switch 自带两个本地技能包定义，不从 Codeg manifest 推导包身份：

```text
ai-switch.core
  用户名称：AI Switch 核心技能包
  成员：核心工作流 Skill ID 列表

ai-switch.science
  用户名称：AI Switch 科学技能包
  成员：科学研究 Skill ID 列表
```

安装状态规则：

- 技能包成员由 AI Switch catalog 的 Skill ID 列表决定。
- 当前 AI Switch 选定 Agent 和作用域的管理目录中存在同 ID Skill 时，该成员显示为已安装。
- 安装技能包只复制当前管理目录中缺失的 ID；同 ID 已存在时跳过，不覆盖已有内容。
- 第一阶段不提供整包卸载，因为无法可靠证明同 ID Skill 的所有权。
- 技能包资源随 AI Switch 应用分发，不读取 Codeg manifest，也不扫描 Codeg 目录。

### 现有 Codex/Agents 来源

保留当前扫描来源：

- `$CODEX_HOME/skills`
- `$CODEX_HOME/skills/.system`
- `$HOME/.agents/skills`

其中 `.system` 标记为内置只读；用户目录中的 Skill 按现有 global/project 规则处理。Codeg 已安装的同 ID Skill 不会被 AI Switch 读取或自动纳入状态，用户在 AI Switch 中安装时会复制到 AI Switch 自己的管理目录，两套目录互不覆盖。

### 分类、标签和中文友好显示

元数据读取顺序如下：

1. `SKILL.md` front matter 的可选字段：`category`、`tags`、`language`、`display_name`。
2. AI Switch 内置技能包 catalog。
3. AI Switch 内置 i18n 映射表。
4. 原始 Skill ID、name 和 description 作为回退。

不修改已有 Skill 文件来写入中文字段。内置映射只负责常见系统 Skill 和两个 AI Switch 包的用户可见名称；未知技能仍可正常展示，只是使用原始文本。

## 后端接口与数据流

保留现有单 Skill 命令：

```text
skills_list_agents()
skills_list(agent_type, scope, workspace_path?)
skills_read(agent_type, scope, skill_id, workspace_path?)
skills_save(agent_type, scope, skill_id, content, layout?, workspace_path?)
skills_delete(agent_type, scope, skill_id, workspace_path?)
```

新增只读包命令：

```text
skills_list_packages(agent_type?, scope?, workspace_path?) -> SkillsPackageListResult
skills_read_package(package_id, agent_type?, scope?, workspace_path?) -> SkillPackageDetail
skills_install_package(package_id, agent_type?, scope?, workspace_path?) -> SkillPackageInstallResult
```

建议结果模型：

```text
SkillsPackageListResult
  packages: SkillPackage[]
  skills: SkillItem[]
  warnings: SkillScanWarning[]

SkillPackageDetail
  package: SkillPackage
  skills: SkillItem[]
```

命令约束：

- 包列表和包详情只读，不进入 Web 敏感写命令门禁。
- 单 Skill 保存/删除和技能包安装继续受 Web 敏感命令门禁保护。
- `skills_list_packages` 应复用单 Skill 扫描的基础函数，不能由前端根据列表自行猜测包关系。
- 所有写操作仍以磁盘上的最终结果为准，完成后重新扫描。
- 包命令失败时返回部分结果和结构化 warning；只在无法读取任何来源时返回整体错误。

## 前端页面结构

### SkillsScreen

`SkillsScreen` 增加内部状态：

```ts
type SkillsView = "skills" | "packages";
```

默认进入 `skills` 子 Tab，不新增顶层路由。切换客户端、作用域和项目路径时，两个子 Tab共享当前筛选上下文，但分别维护自己的选中项。

### “技能”子 Tab

- 保留现有客户端选择、global/project 作用域、项目目录选择、搜索、刷新和新建操作。
- 左侧列表显示名称、ID、来源/包、只读标记和布局。
- 右侧显示 Skill 详情、正文读取、编辑、保存和删除。
- 左右区域分别控制滚动，列表项不能撑大页面网格。

### “技能包”子 Tab

- 左侧显示包名称、来源、版本、安装状态和 Skill 数量。
- 右侧显示包描述、manifest 路径、安装时间、目标客户端和成员 Skill 列表。
- 点击成员 Skill 可跳转到“技能”子 Tab 并选中该 Skill。
- 本期不显示启用/禁用、卸载或远程更新按钮；只提供“安装缺失技能”，且不会覆盖已有同 ID Skill。
- 包扫描空状态应明确说明当前筛选上下文没有可显示的技能包；AI Switch 内置 catalog 正常情况下始终可见。

### Skills 左侧列表布局约束

主容器使用可收缩的双栏网格：

```text
grid min-h-0 min-w-0 grid-cols-[minmax(240px,280px)_minmax(0,1fr)]
```

左栏必须同时具备：

```text
min-h-0 min-w-0 flex flex-col overflow-hidden
```

列表区域使用：

```text
min-h-0 flex-1 overflow-y-auto
```

右栏和正文预览使用 `min-w-0`，长路径和长行允许换行或在自身容器内横向滚动。小于桌面断点时切换为单栏布局，不保留固定 280px 左栏。

`SkillsToolbar` 不再使用“固定四列但渲染五个控件”的网格；改为可收缩的 flex-wrap 或基于实际控件数量的自适应网格，筛选输入允许 `min-w-0`。

## 国际化

当前语言为 `en` 和 `zh-CN`。新增键按模块分组：

```text
nav.mcp
nav.skills
mcp.*
skills.*
skills.package.*
skills.error.*
```

必须覆盖：

- 页面标题、副标题、按钮、表单标签和 aria-label。
- 客户端、作用域、布局、来源、分类、标签和只读状态。
- 加载中、空列表、项目目录缺失、扫描警告和确认提示。
- MCP/Skills 的所有操作错误。

组件不得继续直接写用户可见英文；技术值可以直接显示。包名和常见系统 Skill 名称使用稳定 i18n 映射，未命中的 ID 使用原始值。

## 错误处理

沿用 `ApiClientError.code`，新增包扫描错误码：

```text
skills.package_not_found
skills.package_scan_failed
skills.package_operation_unsupported
```

前端通过错误码选择中英文主提示：

- 已知错误码：显示对应本地化文案。
- 未知错误码：显示模块级通用失败提示。
- `details`：只作为可选技术详情显示，不替代主提示，不输出 Skill 正文、token、API key 或环境变量值。

包扫描遵循部分成功原则：单个 Skill 目录读取失败不阻塞其他技能；无法找到内置资源或目标目录不可写时返回结构化错误。

## 测试与验收

### 前端测试

- MCP 页面在 `en` 和 `zh-CN` 下不出现硬编码用户文案。
- Skills 页面两个子 Tab、切换成员 Skill 和空状态。
- AI Switch 核心包、科学包的列表、安装进度和详情展示。
- API 错误码中英文映射、未知错误回退和 manifest 警告。
- Skills 左栏列表可渲染大量项目，长路径/长正文不会产生页面级横向溢出。
- 客户端/作用域切换清理旧选中状态，单 Skill 保存/删除行为不回归。

### Rust 测试

- front matter 可选字段解析及缺失字段回退。
- AI Switch 内置包 catalog 的包识别、成员列表和同 ID 安装状态。
- 安装技能包时只复制缺失成员，同 ID 已安装时不覆盖。
- package ID、Skill ID 和路径安全校验。
- 包列表复用 Skill 扫描结果并稳定排序、去重。

### 本地检查

```text
pnpm typecheck
pnpm test:run
pnpm rust:check
pnpm rust:test
```

### 手工响应式验收

至少检查 375px、768px、1024px 和 1440px 宽度：

1. Skills 左侧列表不撑破主页面。
2. 长 Skill ID、路径和 Markdown 正文不会导致页面级横向滚动。
3. 桌面端左栏独立纵向滚动，右侧编辑/预览独立滚动。
4. 窄屏端切换为单栏，仍可在列表与详情之间完成选择和编辑。
5. 中英文切换后页面结构和操作状态保持一致。

## 成功标准

1. 顶层设置不出现独立的“技能包”入口；用户可从 `Skills` 进入“技能”和“技能包”。
2. “技能”保留当前单 Skill 管理能力；“技能包”能展示 AI Switch 核心包和科学包，并按同 ID 判断安装状态。
3. MCP、Skills 页面和 API 错误在英文/简体中文下均有完整用户文案。
4. Skills 左侧列表在桌面和窄屏下均不撑破布局。
5. 本期不联网、不提供无实际运行时效果的启用/禁用开关。
6. 既有更新检查工作区改动和旧 MCP/Skills 迁移文档不被覆盖。
