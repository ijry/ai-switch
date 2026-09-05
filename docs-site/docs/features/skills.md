---
title: 技能管理
description: 统一浏览、编辑、安装 11 个 AI CLI 的 Agent Skills，支持全局与项目两种范围、技能目录与单文件两种布局，并内置 27 个技能的两套技能包。
---

# 技能管理

Agent Skills 是一种把可复用工作流交给 AI 智能体的方式：一个技能就是一份 Markdown 说明书，带 YAML frontmatter 描述它什么时候该被用起来。智能体读到这份说明，就知道遇到某类任务时该按什么流程做。

和 [MCP 服务器](/features/mcp) 一样，技能的痛点也在于**每个 CLI 各有一套自己的技能目录**。AI Switch 的技能管理把这些目录统一起来：一个界面里浏览所有客户端的技能、直接编辑内容、把内置技能包一键装进目标 CLI。

在主界面侧边栏「系统」分组下的「技能」进入。

## 技能长什么样

最常见的形态是一个**技能目录**，里面至少有一个 `SKILL.md`：

```text
~/.codex/skills/
└── systematic-debugging/
    ├── SKILL.md
    └── (可选的辅助文件、脚本、模板)
```

`SKILL.md` 的开头是 YAML frontmatter：

```text
---
name: systematic-debugging
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

正文：具体流程说明...
```

`description` 是最关键的一行——智能体靠它判断当前任务是否该调用这个技能。所以描述应该写"什么时候用"，而不是"这是什么"。

另一种形态是**单个 Markdown 文件**（`布局: markdown_file`），技能目录下直接放 `xxx.md`。这个布局**只有 Codex CLI 支持**，其他客户端只认技能目录。

技能 ID 有校验：不能为空，不能是 `.` 或 `..`，不能以 `.` 开头，不能含 `/`、`\`、`:`、空白字符或控制字符。不合法时返回 `skills.invalid_id`。

## 支持的 11 个客户端

各客户端的技能目录（都相对于用户主目录，项目范围则相对于工作区根目录）：

| 客户端 | 全局技能目录 | 项目技能目录（相对工作区） |
| --- | --- | --- |
| Codex CLI | `$CODEX_HOME/skills`、`$CODEX_HOME/skills/.system`（只读）、`~/.agents/skills` | `.codex/skills`、`.agents/skills` |
| Claude Code | `~/.claude/skills` | `.claude/skills` |
| Gemini CLI | `~/.gemini/skills`、`~/.agents/skills` | `.gemini/skills`、`.agents/skills` |
| Grok | `$GROK_HOME/skills` | `.grok/skills` |
| OpenCode | `~/.config/opencode/skills`、`~/.agents/skills` | `.agents/skills`、`.opencode/skills` |
| OpenClaw | `~/.openclaw/skills` | `skills` |
| Hermes Agent | `$HERMES_HOME/skills` | 无 |
| Cline | `~/.agents/skills`、`~/.cline/skills` | `.agents/skills`、`.cline/skills`、`.clinerules/skills`、`.claude/skills` |
| Cursor | `~/.cursor/skills`、`~/.agents/skills`、`~/.cursor/skills-cursor`（只读） | `.cursor/skills`、`.agents/skills` |
| Kimi Code | `$KIMI_CODE_HOME/skills` | `.kimi-code/skills` |
| CodeBuddy | `~/.codebuddy/skills` | `.codebuddy/skills` |

四个客户端支持环境变量覆盖主目录：`CODEX_HOME`、`GROK_HOME`、`HERMES_HOME`、`KIMI_CODE_HOME`。这些变量也支持 `~` 和 `~/xxx` 写法。

几点值得注意：

- **`~/.agents/skills` 是跨工具的共享位置。** Codex、Gemini CLI、OpenCode、Cline、Cursor 五个客户端都会扫它，所以放在这里的技能可以被它们共用，不必复制五份。
- **Codex 的 `$CODEX_HOME/skills/.system` 标记为只读**，那是 Codex 自己维护的系统技能；**Cursor 的 `~/.cursor/skills-cursor` 同样只读**，那是 Cursor 自带的内容。
- **Hermes Agent 没有项目范围目录。** 切到项目范围时它的技能列表是空的。
- **OpenClaw 的项目目录就是工作区根下的 `skills`**，不带点前缀。
- **Cline 的项目范围会一并读 `.claude/skills`**，所以为 Claude Code 提交进仓库的项目技能，Cline 也能直接用。
- **只有 Codex 支持单 Markdown 文件布局**，其余客户端一律用技能目录。

路径解析还带一层越界防护：技能路径规范化之后必须仍落在它所属的存储目录内，否则返回 `skills.path_invalid`。项目范围下工作区目录不存在时返回 `skills.directory_missing`。

## 只读技能

从只读根目录扫出来的技能会带一个「只读」徽标：编辑按钮被禁用（提示"该技能来自只读目录"），删除按钮直接隐藏。

这样做的原因是这些目录归客户端自己管——改了会在升级时被覆盖，甚至可能破坏客户端的自检。想改一个只读技能，把它复制到可写目录下改副本。

## 全局范围与项目范围

技能有两种作用范围：

| 范围 | 位置 | 适用场景 |
| --- | --- | --- |
| 全局（`global`） | 用户主目录下 | 通用工作流：调试方法论、代码评审流程 |
| 项目（`project`） | 工作区目录下 | 项目专属约定：本仓库的发布流程、测试规范 |

默认是全局范围。切到项目范围时**必须先填工作区路径**——路径为空时界面不会发起任何查询，因为"项目范围但不知道是哪个项目"没有意义。

项目技能可以随代码提交进仓库，这样团队成员克隆下来就有同一套技能。

## 界面

技能界面上方是三个选择器：**客户端**（默认 Codex CLI）、**范围**（默认全局）、以及项目范围下的**工作区路径**。下面分两个页签：

### 技能

列出当前客户端 + 范围下扫到的全部技能，每条显示 ID、来源、布局和描述。可以新建、编辑、删除（只读技能除外）。

内置技能包里的 27 个技能，列表和详情标题显示的是 AI Switch 随应用附带的一份中英文文案，不是文件里的 frontmatter——`SKILL.md` 的 `name` 是 kebab-case 的 ID、`description` 是写给智能体看的长触发句，两者都不适合人扫一眼。界面语言切到简体中文时显示中文名和中文一句话说明，英文时显示英文的。搜索框两种语言都能匹配：搜"头脑风暴"和搜 `brainstorming` 找到的是同一个技能。预览和编辑区里始终是文件本身的原文，一个字节都没改。

自己写的技能不受影响，仍然按 frontmatter 的 `name` 和 `description` 显示。

编辑器里有三样东西：

1. **技能 ID**：也就是目录名或文件名。
2. **布局**：技能目录 或 Markdown 文件（后者仅 Codex 可选）。
3. **内容**：`SKILL.md`（或那个 `.md` 文件）的完整正文，含 frontmatter。

技能来源会被标注为内置、Codex、Agents、项目或未知，对应它是从哪个根目录扫出来的。

### 技能包

列出内置的技能包，以及每个包在当前客户端里的安装状态。包详情里既有整包操作，也有单个技能的操作：

- **安装缺失技能**：把包里当前没装的技能一次装完。
- **卸载已安装技能**：把包里已装上的技能一次删完（会先弹确认框）。
- **每个技能一行**，右侧标着「已安装 / 未安装」，跟着一个按钮：未安装的是「安装」，已安装的是「卸载」（同样先确认）。点技能名可以跳回技能页签看它的内容。
- 只读目录里的技能，「卸载」按钮是禁用的，理由和只读技能不能编辑一样。

## 两套内置技能包

AI Switch 随应用打包了两套技能包，共 27 个技能。两者都标记为内置、只读，且**当前只能安装到 Codex CLI**。给其他客户端选技能包页签时列表是空的，尝试安装会返回"AI Switch Skill packages can currently be installed for Codex CLI only"。

### AI Switch Core Skill Pack（`ai-switch.core`，14 个）

包描述：Core agent workflow Skills bundled by AI Switch.

| 技能 | 什么时候用 |
| --- | --- |
| `brainstorming` | 任何创造性工作之前——新功能、新组件、改行为，先探清意图、需求和设计 |
| `dispatching-parallel-agents` | 遇到 2 个以上互不共享状态、无先后依赖的独立任务时 |
| `executing-plans` | 已经有一份书面实施计划，要在独立会话里带评审检查点执行时 |
| `finishing-a-development-branch` | 实现完成、测试全过，需要决定怎么把工作合进主线时 |
| `receiving-code-review` | 收到评审意见、准备动手改之前——尤其是意见看起来不清楚或技术上可疑时，要求技术严谨与核实，而不是敷衍认同或盲从 |
| `requesting-code-review` | 任务完成、大功能实现完、或合并之前，验证是否满足需求 |
| `subagent-driven-development` | 在当前会话里执行含独立任务的实施计划时 |
| `systematic-debugging` | 遇到任何 bug、测试失败或异常行为，在提出修复方案之前 |
| `test-driven-development` | 实现任何功能或修复，在写实现代码之前 |
| `using-git-worktrees` | 开始需要与当前工作区隔离的功能开发时，或执行实施计划之前 |
| `using-superpowers` | 任何对话开始时——建立"如何发现并使用技能"的规范，要求在任何回复（包括反问）之前先调用技能 |
| `verification-before-completion` | 准备声称工作已完成/已修复/已通过，以及提交或建 PR 之前——先跑验证命令确认输出，证据先于断言 |
| `writing-plans` | 拿到多步任务的规格或需求，在动代码之前 |
| `writing-skills` | 创建新技能、修改已有技能、或部署前验证技能是否有效时 |

这一套是**工程流程类**技能：先想清楚再动手、测试驱动、完成前验证、评审要严谨。

### AI Switch Science Skill Pack（`ai-switch.science`，13 个）

包描述：Scientific research and analysis Skills bundled by AI Switch. 全部由 K-Dense Inc. 提供，MIT 许可。

| 技能 | 做什么 |
| --- | --- |
| `citation-management` | 检索 Google Scholar 与 PubMed，抽取准确元数据、校验引用、生成规范 BibTeX |
| `experimental-design` | 数据采集**之前**设计实验：选设计、随机化、区组、排布处理组合 |
| `exploratory-data-analysis` | 对 200 多种科学数据格式做探索性分析，自动识别格式并产出质量报告 |
| `hypothesis-generation` | 从观测或数据出发，结构化地形成可检验假设、提出机制、设计验证实验 |
| `paper-lookup` | 检索 10 个学术 API（PubMed、PMC、bioRxiv、medRxiv、arXiv、OpenAlex、Crossref、Semantic Scholar、CORE、Unpaywall），带可复现的出处 |
| `peer-review` | 按清单写正式同行评审：方法学、统计有效性、报告规范（CONSORT/STROBE）合规性 |
| `scholar-evaluation` | 用 ScholarEval 框架给学术工作打分：问题构造、方法、分析、写作 |
| `scientific-brainstorming` | 开放式研究构想、跨学科关联、挑战假设、找研究空白——适合还没有具体观测的早期阶段 |
| `scientific-critical-thinking` | 评估科学论断与证据质量，识别偏倚与混杂，套用 GRADE、Cochrane 偏倚风险等分级框架 |
| `scientific-schematics` | 生成出版级示意图：神经网络架构、系统图、流程图、生物通路 |
| `scientific-visualization` | 期刊投稿级图表：多面板布局、显著性标注、误差棒、色盲友好配色、各刊格式 |
| `statistical-analysis` | 全流程统计分析：选检验、查假设、算效应量、功效分析、贝叶斯替代方案、APA 格式报告 |
| `statistical-power` | 样本量与统计功效计算：先验功效分析、最小可检测效应、功效曲线，含无闭式解设计的蒙特卡洛仿真 |

这一套技能之间有明确的分工提示，比如 `experimental-design` 负责设计、`statistical-power` 负责算样本量、`statistical-analysis` 负责分析已采集的数据，各自的描述里都写清了该转交给谁。

## 安装行为

点「安装缺失的技能」时：

1. 计算目标客户端当前已有的技能 ID 集合。
2. 只复制**不在这个集合里**的技能。
3. 复制时**只写不存在的文件，绝不覆盖已有文件**。
4. 已经存在的技能 ID 记入"跳过"列表并在结果里回报。

点单个技能的「安装」时是同一套逻辑，只不过范围收窄到那一个 ID。请求里带的 ID 必须是这个包的成员，否则返回 `skills.package_member_missing`——宁可报错，也不要出现"点了安装、提示成功、其实什么都没装"。

::: tip 同名即跳过，与来源无关
判断"已安装"只看技能 ID 匹配，不看它是不是从这个包装进去的。所以你自己写了一个叫 `writing-plans` 的技能，装 Core 包时它会被跳过、原样保留，不会被覆盖。想换成包里的版本，先删掉自己那个再装。
:::

安装目标是当前范围下**第一个可写（非只读）的根目录**。对 Codex 全局范围来说，就是 `$CODEX_HOME/skills`。

## 卸载行为

卸载会**从磁盘删掉技能的文件**（技能目录布局连目录一起删），所以界面上先弹确认框。删除范围只在这个包的成员里：

1. 请求的 ID 必须属于这个包，否则返回 `skills.package_member_missing`。
2. 本来就没装的 ID 记入"跳过"，不算失败——这样"卸载整包"在半装状态下也能一次跑完。
3. 只读目录里的技能同样记入"跳过"，不会去动客户端自己维护的那份。
4. 要删的路径来自技能列表本身，所以删除只可能发生在被扫过的技能目录里。

::: warning 同名即删除，与来源无关
和安装一样，卸载也只按 ID 匹配。如果你自己写了一个叫 `writing-plans` 的技能又点了 Core 包里那一行的「卸载」，删掉的是你那份。删之前看清确认框里的 ID。
:::

## 技能包资源从哪里找

技能包文件随应用一起分发，运行时按以下顺序定位：

1. 环境变量 `AI_SWITCH_SKILL_PACKAGES_DIR`（开发和特殊部署时用来指定目录）。
2. 可执行文件旁的候选目录：`skill-packages`、`resources/skill-packages`、`_up_/skill-packages`、`../skill-packages`、`../resources/skill-packages`。
3. 工作目录相对的候选：`src-tauri/resources/skill-packages`、`resources/skill-packages`、`../src-tauri/resources/skill-packages`。

第 2 组覆盖了三个平台不同的打包布局，第 3 组是为了 `pnpm tauri:dev` 之类的开发场景，见 [本地开发](/dev/local-setup)。

## 生效时机

和 MCP 一样，技能是**客户端启动时加载**的。装完或改完之后，已经在跑的 CLI 进程不会自动感知，需要重启它，或者在 [Vibe 终端](/features/vibe) 里新开一个标签。

## 写自己的技能

1. 界面上选好客户端和范围，点新建。
2. 想一个描述性的 ID，用连字符分词（`deploy-to-staging`，不要 `deploy`）。
3. **`description` 写触发条件。** 对照内置技能的写法——都是 "Use when ..." 开头，说的是"什么情况下该用我"。这一行直接决定智能体会不会想起这个技能。
4. 正文写具体流程。带编号的步骤和明确的判断条件比大段散文有效。
5. 保存后开一个新的 CLI 会话验证：抛一个应该命中该技能的任务，看智能体是否真的按流程走。

想把技能分享给团队，用**项目范围**并提交进仓库；想在多个工具间共用，Codex 用户可以放到 `~/.agents/skills`。

## 下一步

- [MCP 服务器](/features/mcp)：跨客户端配置工具服务器
- [Vibe 终端与皮肤](/features/vibe)：起个新终端验证技能是否被加载
- [会话管理](/features/sessions)：回看智能体实际有没有按技能走
- [平台支持矩阵](/guide/platform-support)：各平台能力对照
